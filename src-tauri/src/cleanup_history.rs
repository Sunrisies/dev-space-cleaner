use huanj_core::{Candidate, CleanResult, Kind};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};

const LIMIT: usize = 1000;
const MAX_BYTES: usize = 20 * 1024 * 1024;
#[derive(Default)]
pub struct CleanupHistoryLock(pub Arc<Mutex<()>>);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    timestamp_ms: u64,
    path: PathBuf,
    bytes: u64,
    kind: Kind,
    success: bool,
    message: String,
}
#[derive(Serialize, Deserialize)]
struct Log {
    version: u8,
    records: Vec<Entry>,
}

pub fn location(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|p| p.join("cleanup-history.json"))
        .map_err(|e| format!("无法确定清理记录位置：{e}"))
}
fn load(path: &Path) -> Result<Log, String> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Ok(Log {
                version: 1,
                records: vec![],
            });
        }
        Err(e) => return Err(format!("无法读取清理记录：{e}")),
    };
    if meta.len() > MAX_BYTES as u64 {
        return Err("清理记录超过大小限制，未覆盖原文件".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("无法读取清理记录：{e}"))?;
    let log: Log =
        serde_json::from_slice(&bytes).map_err(|_| "清理记录损坏，未覆盖原文件".to_string())?;
    if log.version != 1 || log.records.len() > LIMIT {
        return Err("清理记录格式不兼容，未覆盖原文件".into());
    }
    Ok(log)
}
pub fn append(path: &Path, items: &[Candidate], results: &[CleanResult]) -> Result<(), String> {
    if items.len() != results.len() {
        return Err("清理记录数量不匹配".into());
    }
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as u64;
    let mut log = load(path)?;
    for (item, result) in items.iter().zip(results) {
        if item.id != result.id {
            return Err("清理记录标识不匹配".into());
        }
        log.records.push(Entry {
            timestamp_ms,
            path: item.path.clone(),
            bytes: item.bytes,
            kind: item.kind,
            success: result.success,
            message: result.message.clone(),
        });
    }
    if log.records.len() > LIMIT {
        log.records.drain(..log.records.len() - LIMIT);
    }
    let bytes = serde_json::to_vec(&log).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("清理记录超过大小限制，未覆盖原文件".into());
    }
    let parent = path.parent().ok_or("无法确定清理记录目录")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let pending = parent.join(format!(
        ".cleanup-history-{}-{stamp}.tmp",
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(|e| e.to_string())?;
    let result = (|| -> std::io::Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&pending, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(pending);
        return Err(format!("无法保存清理记录，原文件保持不变：{e}"));
    }
    Ok(())
}
#[tauri::command]
pub async fn list_cleanup_history(
    app: AppHandle,
    state: State<'_, CleanupHistoryLock>,
) -> Result<Vec<Entry>, String> {
    let path = location(&app)?;
    let lock = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock.lock().map_err(|_| "清理记录锁异常")?;
        let mut records = load(&path)?.records;
        records.reverse();
        Ok(records)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_recycle_bin() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        #[cfg(windows)]
        {
            std::process::Command::new("explorer.exe")
                .arg("shell:RecycleBinFolder")
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("无法打开回收站：{e}"))
        }
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var_os("HOME").ok_or("无法确定废纸篓位置")?;
            let status = std::process::Command::new("open")
                .arg("-a")
                .arg("Finder")
                .arg(PathBuf::from(home).join(".Trash"))
                .status()
                .map_err(|e| format!("无法打开废纸篓：{e}"))?;
            if status.success() {
                Ok(())
            } else {
                Err("Finder 无法打开废纸篓".into())
            }
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            Err("当前平台不支持打开回收站".into())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_log_is_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cleanup.json");
        assert!(load(&path).unwrap().records.is_empty());
        fs::write(&path, b"{broken").unwrap();
        assert!(append(&path, &[], &[]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken");
    }
    #[test]
    fn records_success_and_failure_and_limits_history() {
        use huanj_core::{ScanEvent, ScanOptions, scan};
        use std::sync::{Arc, atomic::AtomicBool};
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("Cargo.toml"), "[workspace]").unwrap();
        fs::create_dir_all(temp.path().join("target")).unwrap();
        fs::write(temp.path().join("target/data"), [0u8; 37]).unwrap();
        let item = Mutex::new(None);
        scan(
            ScanOptions {
                roots: vec![temp.path().into()],
                kinds: vec![Kind::Rust],
                workers: 1,
            },
            Arc::new(AtomicBool::new(false)),
            |event| {
                if let ScanEvent::Sized(found) = event {
                    *item.lock().unwrap() = Some(found);
                }
            },
        );
        let item = item.into_inner().unwrap().unwrap();
        let path = temp.path().join("cleanup.json");
        let success = CleanResult {
            id: item.id,
            success: true,
            message: "已移入回收站".into(),
            history_error: None,
        };
        let failure = CleanResult {
            id: item.id,
            success: false,
            message: "目录已变化".into(),
            history_error: None,
        };
        append(&path, std::slice::from_ref(&item), &[success]).unwrap();
        append(&path, std::slice::from_ref(&item), &[failure]).unwrap();
        let log = load(&path).unwrap();
        assert_eq!(log.records.len(), 2);
        assert!(log.records[0].success);
        assert_eq!(
            (log.records[0].bytes, log.records[0].path.clone()),
            (37, item.path.clone())
        );
        assert!(!log.records[1].success);
        assert_eq!(log.records[1].message, "目录已变化");
        let items = vec![item.clone(); LIMIT + 1];
        let results = (0..items.len())
            .map(|_| CleanResult {
                id: item.id,
                success: true,
                message: "ok".into(),
                history_error: None,
            })
            .collect::<Vec<_>>();
        append(&path, &items, &results).unwrap();
        assert_eq!(load(&path).unwrap().records.len(), LIMIT);
    }
}
