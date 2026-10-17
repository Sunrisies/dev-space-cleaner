use huanj_core::{Candidate, is_link};
use std::{fs, path::PathBuf};

pub fn resolve(item: &Candidate) -> Result<PathBuf, String> {
    if !item.path.starts_with(&item.root) {
        return Err("目录已超出扫描范围，请重新扫描".into());
    }
    for path in item.path.ancestors() {
        let meta = fs::symlink_metadata(path)
            .map_err(|e| format!("目录不可访问，可能已移动或清理：{e}"))?;
        if !meta.is_dir() || is_link(&meta) {
            return Err("目录或上级路径已变成链接，请重新扫描".into());
        }
        if path == item.root {
            break;
        }
    }
    let path = fs::canonicalize(&item.path).map_err(|e| e.to_string())?;
    if path != item.path {
        return Err("目录实际位置已变化，请重新扫描".into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use huanj_core::{Kind, ScanEvent, ScanOptions, scan};
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    #[test]
    fn permits_registered_directory_before_sizing_but_rejects_removed_or_outside_paths() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Cargo.toml"), "[workspace]").unwrap();
        fs::create_dir(root.path().join("target")).unwrap();
        let candidate = Mutex::new(None);
        scan(
            ScanOptions {
                roots: vec![root.path().into()],
                kinds: vec![Kind::Rust],
                workers: 1,
            },
            Arc::new(AtomicBool::new(false)),
            |event| {
                if let ScanEvent::Found(item) = event {
                    *candidate.lock().unwrap() = Some(item);
                }
            },
        );
        let mut item = candidate.into_inner().unwrap().unwrap();
        assert!(!item.measured);
        assert!(resolve(&item).is_ok());
        let original = item.root.clone();
        item.root = root.path().join("unrelated");
        assert!(resolve(&item).is_err());
        item.root = original;
        fs::remove_dir(&item.path).unwrap();
        assert!(resolve(&item).is_err());
    }
}
