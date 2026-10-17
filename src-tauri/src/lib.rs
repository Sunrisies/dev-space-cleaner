use huanj_core::{
    Candidate, CleanResult, Drive, Kind, ScanEvent, ScanOptions, clean, drives, scan,
};
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
}
#[derive(Default)]
struct AppState(Arc<Mutex<Session>>);

#[tauri::command]
fn list_drives() -> Vec<Drive> {
    drives()
}

#[tauri::command]
fn start_scan(
    session: String,
    roots: Option<Vec<String>>,
    kinds: Vec<String>,
    workers: usize,
    on_event: Channel<Envelope>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let kinds: Vec<Kind> = kinds
        .iter()
        .filter_map(|k| match k.as_str() {
            "node" => Some(Kind::Node),
            "rust" => Some(Kind::Rust),
            "python" => Some(Kind::Python),
            _ => None,
        })
        .collect();
    if kinds.is_empty() {
        return Err("至少选择一种目录类型".into());
    }
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
        };
    }
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let roots: Vec<PathBuf> = roots
                .map(|r| r.into_iter().map(PathBuf::from).collect())
                .unwrap_or_else(|| drives().into_iter().map(|d| d.path).collect());
            scan(
                ScanOptions {
                    roots,
                    kinds,
                    workers: workers.clamp(1, 16),
                },
                cancel.clone(),
                |event| {
                    // Commit complete candidates before sending the event to the UI.
                    if let ScanEvent::Sized(ref item) = event {
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
                },
            )
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
    session: String,
    ids: Vec<u64>,
    state: State<'_, AppState>,
) -> Result<Vec<CleanResult>, String> {
    let shared = state.0.clone();
    let items = {
        let mut current = shared.lock().map_err(|e| e.to_string())?;
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
        items
    };
    let result =
        tauri::async_runtime::spawn_blocking(move || items.iter().map(clean).collect::<Vec<_>>())
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_drives,
            start_scan,
            cancel_scan,
            clean_selected
        ])
        .run(tauri::generate_context!())
        .expect("无法启动缓境");
}
