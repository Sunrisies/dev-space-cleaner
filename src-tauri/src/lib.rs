mod cleanup_history;
mod directory;
mod history;
mod preferences;
use huanj_core::{
    Candidate, CleanResult, Drive, Kind, ScanEvent, ScanOptions, clean, drives, refresh_one, scan,
    scan_global_caches, validate_candidate,
};
use preferences::PreferencesLock;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{State, ipc::Channel};
use tauri_plugin_opener::OpenerExt;

#[derive(Clone, Serialize)]
struct Envelope {
    session: String,
    event: ScanEvent,
}

#[derive(Default)]
struct Session {
    id: String,
    busy: bool,
    cancel: Arc<AtomicBool>,
    candidates: HashMap<u64, Candidate>,
    history_mode: bool,
}
#[derive(Default)]
struct AppState(Arc<Mutex<Session>>);

#[tauri::command]
async fn refresh_candidate(
    session: String,
    id: u64,
    path: String,
    kind: String,
    rule: String,
    state: State<'_, AppState>,
) -> Result<Candidate, String> {
    if session.is_empty() || rule.is_empty() {
        return Err("缺少扫描会话或目录识别规则".into());
    }
    let kind = Kind::from_id(&kind).ok_or("未知扫描类型")?;
    let path = PathBuf::from(path);
    let shared = state.0.clone();
    {
        let mut current = shared.lock().map_err(|e| e.to_string())?;
        if current.busy {
            return Err("当前任务尚未结束".into());
        }
        if current.id.is_empty() {
            current.id = session.clone();
            current.history_mode = true;
        }
        if current.id != session {
            return Err("扫描会话已过期，请重新扫描".into());
        }
        if !current.history_mode {
            let original = current
                .candidates
                .get(&id)
                .ok_or("候选目录不存在，请重新扫描")?;
            if original.path != path || original.kind != kind || original.rule != rule {
                return Err("候选目录身份不匹配，请重新扫描".into());
            }
        }
        current.candidates.remove(&id);
        current.busy = true;
    }
    let result =
        tauri::async_runtime::spawn_blocking(move || refresh_one(path, id, kind, &rule)).await;
    let mut current = shared.lock().map_err(|e| e.to_string())?;
    current.busy = false;
    let item = result.map_err(|e| format!("刷新任务失败：{e}"))??;
    current.candidates.insert(id, item.clone());
    Ok(item)
}

fn take_selection(
    current: &mut Session,
    session: &str,
    ids: Vec<u64>,
) -> Result<Vec<Candidate>, String> {
    if current.busy {
        return Err("当前任务尚未结束".into());
    }
    if current.id != session {
        return Err("扫描会话已过期，请重新扫描".into());
    }
    if ids.is_empty() {
        return Err("请先选择目录".into());
    }
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for id in ids {
        if !seen.insert(id) {
            continue;
        }
        let item = current
            .candidates
            .get(&id)
            .ok_or("候选目录不存在，请重新扫描")?;
        if items.iter().any(|other: &Candidate| {
            item.path.starts_with(&other.path) || other.path.starts_with(&item.path)
        }) {
            return Err("所选目录范围重叠，请重新扫描".into());
        }
        items.push(item.clone());
    }
    current.busy = true;
    Ok(items)
}

#[derive(Serialize)]
struct PreflightResult {
    id: u64,
    valid: bool,
    message: String,
}

#[tauri::command]
async fn preview_selected(
    session: String,
    ids: Vec<u64>,
    state: State<'_, AppState>,
) -> Result<Vec<PreflightResult>, String> {
    let shared = state.0.clone();
    let items = {
        let mut current = shared.lock().map_err(|e| e.to_string())?;
        take_selection(&mut current, &session, ids)?
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        items
            .iter()
            .map(|item| match validate_candidate(item) {
                Ok(()) => PreflightResult {
                    id: item.id,
                    valid: true,
                    message: "识别依据与目录身份校验通过".into(),
                },
                Err(message) => PreflightResult {
                    id: item.id,
                    valid: false,
                    message,
                },
            })
            .collect::<Vec<_>>()
    })
    .await;
    shared.lock().map_err(|e| e.to_string())?.busy = false;
    result.map_err(|e| format!("清理前检查失败：{e}"))
}

#[tauri::command]
fn list_drives() -> Vec<Drive> {
    drives()
}

#[tauri::command]
async fn list_global_caches() -> Result<Vec<huanj_core::global_cache::GlobalCache>, String> {
    tauri::async_runtime::spawn_blocking(huanj_core::global_cache::discover_system)
        .await
        .map_err(|e| format!("无法发现全局缓存：{e}"))
}

#[tauri::command]
fn start_scan(
    session: String,
    roots: Option<Vec<String>>,
    kinds: Vec<String>,
    global_ids: Option<Vec<String>>,
    workers: usize,
    on_event: Channel<Envelope>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let kinds: Vec<Kind> = kinds
        .iter()
        .map(|k| Kind::from_id(k).ok_or_else(|| format!("未知扫描类型：{k}")))
        .collect::<Result<_, _>>()?;
    if kinds.is_empty() && global_ids.is_none() {
        return Err("至少选择一种目录类型".into());
    }
    let global = if let Some(ids) = global_ids {
        let requested: HashSet<_> = ids.into_iter().collect();
        if requested.is_empty() {
            return Err("请先选择全局缓存".into());
        }
        let found: Vec<_> = huanj_core::global_cache::discover_system()
            .into_iter()
            .filter(|c| requested.contains(&c.id))
            .collect();
        if found.len() != requested.len() {
            return Err("所选缓存已变化或不可访问，请重新检查全局缓存".into());
        }
        Some(found)
    } else {
        None
    };
    if session.is_empty() {
        return Err("缺少扫描会话标识".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let shared = state.0.clone();
    {
        let mut current = shared.lock().map_err(|e| e.to_string())?;
        if current.busy {
            return Err("请等待当前任务结束".into());
        }
        *current = Session {
            id: session.clone(),
            busy: true,
            cancel: cancel.clone(),
            candidates: HashMap::new(),
            history_mode: false,
        };
    }
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let roots: Vec<PathBuf> = roots
                .map(|r| r.into_iter().map(PathBuf::from).collect())
                .unwrap_or_else(|| drives().into_iter().map(|d| d.path).collect());
            let sink = |event| {
                // Register paths before displaying them. Sizing replaces the
                // snapshot; cleaning still requires a complete measurement.
                if let ScanEvent::Found(ref item) | ScanEvent::Sized(ref item) = event {
                    let mut current = shared.lock().unwrap();
                    if current.id == session {
                        current.candidates.insert(item.id, item.clone());
                    }
                }
                // Finished is emitted below only after clearing busy, preventing an immediate restart race.
                if !matches!(event, ScanEvent::Finished(_))
                    && on_event
                        .send(Envelope {
                            session: session.clone(),
                            event,
                        })
                        .is_err()
                {
                    cancel.store(true, Ordering::Relaxed);
                }
            };
            if let Some(global) = global {
                scan_global_caches(global, workers, cancel.clone(), sink)
            } else {
                scan(
                    ScanOptions {
                        roots,
                        kinds,
                        workers: workers.clamp(1, 16),
                    },
                    cancel.clone(),
                    sink,
                )
            }
        }));
        if let Ok(mut current) = shared.lock() {
            current.busy = false;
        }
        match result {
            Ok(summary) => {
                let _ = on_event.send(Envelope {
                    session,
                    event: ScanEvent::Finished(summary),
                });
            }
            Err(_) => {
                let _ = on_event.send(Envelope {
                    session: session.clone(),
                    event: ScanEvent::Issue {
                        path: PathBuf::new(),
                        message: "扫描线程发生异常，任务已停止，请重新扫描".into(),
                    },
                });
                let _ = on_event.send(Envelope {
                    session,
                    event: ScanEvent::Finished(huanj_core::ScanSummary {
                        progress: Default::default(),
                        cancelled: true,
                        elapsed_ms: 0,
                        roots: vec![],
                    }),
                });
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel_scan(session: String, state: State<'_, AppState>) -> Result<(), String> {
    let current = state.0.lock().map_err(|e| e.to_string())?;
    if current.id != session {
        return Err("扫描会话已过期".into());
    }
    current.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
async fn clean_selected(
    app: tauri::AppHandle,
    session: String,
    ids: Vec<u64>,
    state: State<'_, AppState>,
    log_state: State<'_, cleanup_history::CleanupHistoryLock>,
) -> Result<Vec<CleanResult>, String> {
    let shared = state.0.clone();
    let items = {
        let mut current = shared.lock().map_err(|e| e.to_string())?;
        take_selection(&mut current, &session, ids)?
    };
    let log_path = cleanup_history::location(&app);
    let log_lock = log_state.0.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut results = items.iter().map(clean).collect::<Vec<_>>();
        let written = log_path.and_then(|path| {
            let _guard = log_lock.lock().map_err(|_| "清理记录锁异常".to_string())?;
            cleanup_history::append(&path, &items, &results)
        });
        if let Err(error) = written {
            for result in &mut results {
                result.history_error = Some(error.clone());
            }
        }
        results
    })
    .await;
    let mut current = shared.lock().map_err(|e| e.to_string())?;
    current.busy = false;
    let results = result.map_err(|e| format!("清理任务异常：{e}"))?;
    for result in &results {
        if result.success {
            current.candidates.remove(&result.id);
        }
    }
    Ok(results)
}

#[tauri::command]
async fn open_candidate_directory(
    app: tauri::AppHandle,
    session: String,
    id: u64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let item = {
        let current = state.0.lock().map_err(|e| e.to_string())?;
        if current.id != session {
            return Err("扫描会话已过期，请重新扫描".into());
        }
        current
            .candidates
            .get(&id)
            .cloned()
            .ok_or("目录已清理或不属于本次扫描")?
    };
    tauri::async_runtime::spawn_blocking(move || {
        let path = directory::resolve(&item)?;
        let path = dunce::simplified(&path)
            .to_str()
            .ok_or("目录路径编码无法被文件管理器识别")?;
        app.opener()
            .open_path(path, None::<&str>)
            .map_err(|e| format!("无法打开目录：{e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .manage(PreferencesLock::default())
        .manage(history::HistoryLock::default())
        .manage(cleanup_history::CleanupHistoryLock::default())
        .invoke_handler(tauri::generate_handler![
            list_drives,
            list_global_caches,
            start_scan,
            cancel_scan,
            preview_selected,
            refresh_candidate,
            clean_selected,
            cleanup_history::list_cleanup_history,
            cleanup_history::open_recycle_bin,
            open_candidate_directory,
            preferences::load_preferences,
            preferences::save_preferences,
            history::load_scan_history,
            history::save_scan_history
        ])
        .run(tauri::generate_context!())
        .expect("无法启动缓境");
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use std::fs;
    #[test]
    fn preflight_reserves_session_and_rejects_unknown_or_overlapping_items() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("package.json"), "{}").unwrap();
        fs::create_dir(temp.path().join("node_modules")).unwrap();
        let found = Mutex::new(None);
        scan(
            ScanOptions {
                roots: vec![temp.path().into()],
                kinds: vec![Kind::Node],
                workers: 1,
            },
            Arc::new(AtomicBool::new(false)),
            |event| {
                if let ScanEvent::Sized(item) = event {
                    *found.lock().unwrap() = Some(item);
                }
            },
        );
        let item = found.into_inner().unwrap().unwrap();
        let mut nested = item.clone();
        nested.id = item.id + 1;
        nested.path = item.path.join("nested");
        let mut state = Session {
            id: "current".into(),
            candidates: [(item.id, item.clone()), (nested.id, nested.clone())].into(),
            ..Default::default()
        };
        assert!(take_selection(&mut state, "wrong", vec![item.id]).is_err());
        assert!(take_selection(&mut state, "current", vec![999]).is_err());
        assert!(take_selection(&mut state, "current", vec![item.id, nested.id]).is_err());
        let selected = take_selection(&mut state, "current", vec![item.id, item.id]).unwrap();
        assert_eq!(selected.len(), 1);
        assert!(take_selection(&mut state, "current", vec![item.id]).is_err());
        assert!(validate_candidate(&selected[0]).is_ok());
        fs::remove_file(temp.path().join("package.json")).unwrap();
        assert!(validate_candidate(&selected[0]).is_err());
    }
}
