use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    All,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    version: u8,
    mode: Scope,
    roots: Vec<String>,
    kinds: Vec<String>,
    workers: usize,
}

impl Preferences {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("扫描设置版本不兼容，请重新设置后保存".into());
        }
        if !(1..=16).contains(&self.workers) {
            return Err("工作线程数应为 1–16".into());
        }
        if self.kinds.is_empty()
            || self
                .kinds
                .iter()
                .any(|k| huanj_core::Kind::from_id(k).is_none())
        {
            return Err("请至少选择一种有效的扫描类型".into());
        }
        if self.mode == Scope::Custom && self.roots.is_empty() {
            return Err("请先添加自定义扫描目录".into());
        }
        if self
            .roots
            .iter()
            .any(|p| p.is_empty() || !Path::new(p).is_absolute())
        {
            return Err("扫描目录必须是完整的绝对路径".into());
        }
        // Keep unavailable drives: they may be plugged back in before the next scan.
        Ok(())
    }
}

fn load(path: &Path) -> Result<Option<Preferences>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("无法读取扫描设置：{e}")),
    };
    let settings: Preferences = serde_json::from_slice(&bytes)
        .map_err(|_| "保存的扫描设置内容损坏，请重新设置后保存".to_string())?;
    settings.validate()?;
    Ok(Some(settings))
}

fn save(path: &Path, settings: &Preferences) -> Result<(), String> {
    settings.validate()?;
    let parent = path.parent().ok_or("无法确定扫描设置保存位置")?;
    fs::create_dir_all(parent).map_err(|e| format!("无法创建设置目录：{e}"))?;
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let pending = parent.join(format!(".scan-settings-{}-{stamp}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(|e| format!("无法保存扫描设置，原设置保持不变：{e}"))?;
    let result = (|| -> std::io::Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&pending, path)?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&pending);
        return Err(format!("无法保存扫描设置，原设置保持不变：{e}"));
    }
    Ok(())
}

#[derive(Default)]
pub struct PreferencesLock(Arc<Mutex<()>>);

fn location(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|p| p.join("scan-settings.json"))
        .map_err(|e| format!("无法确定设置目录：{e}"))
}

#[tauri::command]
pub async fn load_preferences(
    app: AppHandle,
    state: State<'_, PreferencesLock>,
) -> Result<Option<Preferences>, String> {
    let path = location(&app)?;
    let lock = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock.lock().map_err(|_| "扫描设置锁异常".to_string())?;
        load(&path)
    })
    .await
    .map_err(|e| format!("读取扫描设置失败：{e}"))?
}

#[tauri::command]
pub async fn save_preferences(
    app: AppHandle,
    settings: Preferences,
    state: State<'_, PreferencesLock>,
) -> Result<(), String> {
    let path = location(&app)?;
    let lock = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock.lock().map_err(|_| "扫描设置锁异常".to_string())?;
        save(&path, &settings)
    })
    .await
    .map_err(|e| format!("保存扫描设置失败：{e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn example(root: &Path) -> Preferences {
        Preferences {
            version: 1,
            mode: Scope::Custom,
            roots: vec![root.join("暂未挂载的项目").to_string_lossy().into_owned()],
            kinds: vec!["node".into(), "python".into()],
            workers: 8,
        }
    }
    #[test]
    fn restores_custom_roots_and_atomically_replaces_previous_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/scan-settings.json");
        assert_eq!(load(&path).unwrap(), None);
        let mut settings = example(dir.path());
        save(&path, &settings).unwrap();
        assert_eq!(load(&path).unwrap(), Some(settings.clone()));
        settings.mode = Scope::All;
        settings.workers = 2;
        save(&path, &settings).unwrap();
        assert_eq!(load(&path).unwrap(), Some(settings));
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }
    #[test]
    fn invalid_settings_do_not_overwrite_saved_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scan-settings.json");
        let settings = example(dir.path());
        save(&path, &settings).unwrap();
        let mut invalid = settings.clone();
        invalid.roots.clear();
        assert!(save(&path, &invalid).is_err());
        assert_eq!(load(&path).unwrap(), Some(settings));
    }
    #[test]
    fn expanded_types_round_trip_without_expanding_old_selections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scan-settings.json");
        let old = example(dir.path());
        save(&path, &old).unwrap();
        assert_eq!(load(&path).unwrap().unwrap().kinds, old.kinds);
        let mut expanded = old;
        expanded.kinds = huanj_core::Kind::all()
            .into_iter()
            .map(|kind| {
                serde_json::to_value(kind)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        save(&path, &expanded).unwrap();
        assert_eq!(load(&path).unwrap(), Some(expanded.clone()));
        expanded.kinds.push("unknown".into());
        assert!(save(&path, &expanded).is_err());
        assert_eq!(load(&path).unwrap().unwrap().kinds.len(), 16);
    }
    #[test]
    fn corrupt_settings_are_reported_without_modifying_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scan-settings.json");
        fs::write(&path, b"{corrupt").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{corrupt");
    }
    #[test]
    fn failed_replacement_preserves_destination_and_removes_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scan-settings.json");
        fs::create_dir(&path).unwrap();
        assert!(save(&path, &example(dir.path())).is_err());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
