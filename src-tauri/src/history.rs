use serde_json::Value;
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};

const MAX_BYTES: usize = 20 * 1024 * 1024;
#[derive(Default)]
pub struct HistoryLock(Arc<Mutex<()>>);

fn validate(value: &Value) -> Result<(), String> {
    if value.get("version").and_then(Value::as_u64) != Some(1)
        || !value.get("saved_at").is_some_and(Value::is_number)
        || !value
            .get("items")
            .and_then(Value::as_array)
            .is_some_and(|a| a.len() <= 100_000)
        || !value.get("summary").is_some_and(Value::is_object)
        || !value.get("scan_kinds").is_some_and(Value::is_array)
    {
        return Err("历史扫描记录格式无效，请重新扫描".into());
    }
    Ok(())
}
fn location(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|p| p.join("scan-history.json"))
        .map_err(|e| format!("无法确定历史记录位置：{e}"))
}
fn load(path: &Path) -> Result<Option<Value>, String> {
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("无法读取历史记录：{e}")),
    };
    if metadata.len() > MAX_BYTES as u64 {
        return Err("历史记录超过大小限制，请重新扫描".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("无法读取历史记录：{e}"))?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "历史记录内容损坏，请重新扫描".to_string())?;
    validate(&value)?;
    Ok(Some(value))
}
fn save(path: &Path, value: &Value) -> Result<(), String> {
    validate(value)?;
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("历史记录超过大小限制，原记录保持不变".into());
    }
    let parent = path.parent().ok_or("无法确定历史记录目录")?;
    fs::create_dir_all(parent).map_err(|e| format!("无法创建历史记录目录：{e}"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let pending = parent.join(format!(".scan-history-{}-{stamp}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(|e| format!("无法保存历史记录：{e}"))?;
    let result = (|| -> std::io::Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&pending, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&pending);
        return Err(format!("无法保存历史记录，原记录保持不变：{e}"));
    }
    Ok(())
}
#[tauri::command]
pub async fn load_scan_history(
    app: AppHandle,
    state: State<'_, HistoryLock>,
) -> Result<Option<Value>, String> {
    let path = location(&app)?;
    let lock = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock.lock().map_err(|_| "历史记录锁异常")?;
        load(&path)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn save_scan_history(
    app: AppHandle,
    snapshot: Value,
    state: State<'_, HistoryLock>,
) -> Result<(), String> {
    let path = location(&app)?;
    let lock = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock.lock().map_err(|_| "历史记录锁异常")?;
        save(&path, &snapshot)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_snapshot_round_trips_and_invalid_input_preserves_previous_record() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("history.json");
        assert_eq!(load(&path).unwrap(), None);
        let valid = serde_json::json!({"version":1,"saved_at":1234,"items":[],"summary":{},"scan_kinds":["node"]});
        save(&path, &valid).unwrap();
        assert_eq!(load(&path).unwrap(), Some(valid.clone()));
        assert!(save(&path, &serde_json::json!({"version":2})).is_err());
        assert_eq!(load(&path).unwrap(), Some(valid));
        fs::write(&path, b"{broken").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(path).unwrap(), b"{broken");
    }
}
