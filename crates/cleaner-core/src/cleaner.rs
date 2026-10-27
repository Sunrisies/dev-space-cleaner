use crate::{Candidate, is_link, rules::identify};
use serde::Serialize;
use std::fs;

#[derive(Debug, Serialize)]
pub struct CleanResult {
    pub id: u64,
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_error: Option<String>,
}

pub fn validate(item: &Candidate) -> Result<(), String> {
    if !item.measured || !item.complete {
        return Err("目录大小未完整统计，请重新扫描".into());
    }
    if item.path == item.root || !item.path.starts_with(&item.root) {
        return Err("拒绝清理扫描根或范围外目录".into());
    }
    for path in item.path.ancestors() {
        let meta = fs::symlink_metadata(path).map_err(|e| format!("重新校验失败：{e}"))?;
        if is_link(&meta) || !meta.is_dir() {
            return Err("目录或上级路径包含链接，已拒绝清理".into());
        }
        if path == item.path && meta.created().ok() != item.created {
            return Err("目录已被替换，请重新扫描".into());
        }
        if path == item.root {
            break;
        }
    }
    let actual = fs::canonicalize(&item.path).map_err(|e| e.to_string())?;
    if actual != item.path {
        return Err("目录实际路径已变化".into());
    }
    let valid_rule = if let Some(id) = item.rule.strip_prefix("global:") {
        crate::global_cache::validates(id, &item.path)
    } else {
        identify(&item.path).is_some_and(|r| r.kind == item.kind && r.rule == item.rule)
    };
    if !valid_rule {
        return Err("项目识别依据已变化，请重新扫描".into());
    }
    Ok(())
}

pub fn clean(item: &Candidate) -> CleanResult {
    let outcome = validate(item).and_then(|_| recycle(&item.path));
    match outcome {
        Ok(()) => CleanResult {
            id: item.id,
            success: true,
            message: "已移入回收站；清空回收站后才会释放空间".into(),
            history_error: None,
        },
        Err(message) => CleanResult {
            id: item.id,
            success: false,
            message,
            history_error: None,
        },
    }
}

fn recycle(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        crate::windows_trash::recycle(path)
    }
    #[cfg(not(windows))]
    {
        trash::delete(path).map_err(|e| format!("移入回收站失败：{e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Kind, ScanEvent, ScanOptions, scan};
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    #[test]
    fn rejects_changed_evidence_and_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        let items = Mutex::new(Vec::new());
        scan(
            ScanOptions {
                roots: vec![dir.path().into()],
                kinds: vec![Kind::Node],
                workers: 2,
            },
            Arc::new(AtomicBool::new(false)),
            |e| {
                if let ScanEvent::Sized(c) = e {
                    items.lock().unwrap().push(c);
                }
            },
        );
        let mut item = items.into_inner().unwrap().pop().unwrap();
        assert!(validate(&item).is_ok());
        fs::remove_file(dir.path().join("package.json")).unwrap();
        assert!(validate(&item).is_err());
        item.root = item.path.clone();
        assert!(validate(&item).is_err());
    }
    #[test]
    fn refreshing_one_directory_remeasures_and_rejects_changed_rule() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[workspace]").unwrap();
        fs::create_dir_all(dir.path().join("target/debug")).unwrap();
        fs::write(dir.path().join("target/debug/item"), [0u8; 41]).unwrap();
        let target = dir.path().canonicalize().unwrap().join("target");
        let item = crate::refresh_one(target.clone(), 45, Kind::Rust, "cargo-project").unwrap();
        assert_eq!(
            (item.id, item.bytes, item.files, item.complete),
            (45, 41, 1, true)
        );
        assert!(validate(&item).is_ok());
        assert!(crate::refresh_one(target.clone(), 45, Kind::Java, "cargo-project").is_err());
        fs::remove_file(dir.path().join("Cargo.toml")).unwrap();
        assert!(crate::refresh_one(target, 45, Kind::Rust, "cargo-project").is_err());
    }
}
