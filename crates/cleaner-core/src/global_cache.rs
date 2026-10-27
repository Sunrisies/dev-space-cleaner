//! Conservative discovery of known global-cache locations. Discovery only reads metadata.
use crate::platform::is_link;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Mac,
}

#[derive(Debug, Clone)]
pub struct Locations {
    pub home: PathBuf,
    pub local_cache: PathBuf,
    pub platform: Platform,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GlobalCache {
    pub id: String,
    pub tool: String,
    pub path: PathBuf,
    pub category: &'static str,
    pub impact: &'static str,
}

pub fn system_locations() -> Option<Locations> {
    #[cfg(windows)]
    {
        Some(Locations {
            home: std::env::var_os("USERPROFILE")?.into(),
            local_cache: std::env::var_os("LOCALAPPDATA")?.into(),
            platform: Platform::Windows,
        })
    }
    #[cfg(target_os = "macos")]
    {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        Some(Locations {
            local_cache: home.join("Library/Caches"),
            home,
            platform: Platform::Mac,
        })
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}

pub fn discover_system() -> Vec<GlobalCache> {
    system_locations()
        .map(|locations| discover(&locations))
        .unwrap_or_default()
}

pub(crate) fn validates(id: &str, path: &Path) -> bool {
    system_locations().is_some_and(|locations| validates_in(&locations, id, path))
}

fn validates_in(locations: &Locations, id: &str, path: &Path) -> bool {
    discover(locations)
        .into_iter()
        .any(|entry| entry.id == id && fs::canonicalize(entry.path).is_ok_and(|p| p == path))
}

// Check each ancestor separately: canonicalization alone would follow junctions.
fn plain_directory(path: &Path) -> bool {
    path.is_absolute()
        && path
            .ancestors()
            .all(|p| fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && !is_link(&m)))
}

fn add(
    out: &mut Vec<GlobalCache>,
    id: &str,
    tool: &str,
    path: PathBuf,
    category: &'static str,
    impact: &'static str,
    markers: &[&str],
) {
    if !plain_directory(&path)
        || !markers.iter().all(|name| {
            let marker = path.join(name);
            fs::symlink_metadata(marker).is_ok_and(|m| !is_link(&m) && m.is_dir())
        })
    {
        return;
    }
    out.push(GlobalCache {
        id: id.into(),
        tool: tool.into(),
        path,
        category,
        impact,
    });
}

/// Finds only standard locations, never runs developer tools or reads their config files.
pub fn discover(locations: &Locations) -> Vec<GlobalCache> {
    let mut result = Vec::new();
    let npm = match locations.platform {
        Platform::Windows => locations.local_cache.join("npm-cache/_cacache"),
        Platform::Mac => locations.home.join(".npm/_cacache"),
    };
    add(
        &mut result,
        "npm-cache",
        "npm",
        npm,
        "共享下载缓存",
        "之后安装可能需要重新联网下载",
        &["content-v2", "index-v5"],
    );
    let pip = match locations.platform {
        Platform::Windows => locations.local_cache.join("pip/Cache"),
        Platform::Mac => locations.home.join("Library/Caches/pip"),
    };
    for (id, folder) in [("pip-http", "http-v2"), ("pip-wheels", "wheels")] {
        add(
            &mut result,
            id,
            "pip",
            pip.join(folder),
            "共享下载缓存",
            "之后安装可能需要重新下载或构建",
            &[],
        );
    }
    let go = match locations.platform {
        Platform::Windows => locations.local_cache.join("go-build"),
        Platform::Mac => locations.home.join("Library/Caches/go-build"),
    };
    add(
        &mut result,
        "go-build",
        "Go",
        go,
        "构建缓存",
        "之后构建需要重新编译",
        &["00", "01"],
    );
    add(
        &mut result,
        "go-download",
        "Go",
        locations.home.join("go/pkg/mod/cache/download"),
        "共享下载缓存",
        "多个项目可能需要重新联网下载依赖",
        &[],
    );
    let gradle = locations.home.join(".gradle/caches");
    if plain_directory(&gradle)
        && let Ok(entries) = fs::read_dir(&gradle)
    {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("build-cache-")
                && name[12..].chars().all(|c| c.is_ascii_digit())
                && name.len() > 12
            {
                add(
                    &mut result,
                    &format!("gradle-{name}"),
                    "Gradle",
                    entry.path(),
                    "构建缓存",
                    "之后构建需要重新生成缓存",
                    &[],
                );
            }
        }
    }
    let pnpm = match locations.platform {
        Platform::Windows => locations.local_cache.join("pnpm/store"),
        Platform::Mac => locations.home.join("Library/pnpm/store"),
    };
    if plain_directory(&pnpm)
        && let Ok(entries) = fs::read_dir(&pnpm)
    {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('v')
                && name.len() > 1
                && name[1..].chars().all(|c| c.is_ascii_digit())
            {
                add(
                    &mut result,
                    &format!("pnpm-{name}"),
                    "pnpm",
                    entry.path(),
                    "共享下载缓存",
                    "多个项目可能需要重新联网下载依赖；硬链接影响实际释放空间",
                    &["files"],
                );
            }
        }
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_specific_caches_and_preserves_unrelated_directories() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let local = root.join("local");
        for path in [
            local.join("npm-cache/_cacache/content-v2"),
            local.join("npm-cache/_cacache/index-v5"),
            local.join("pip/Cache/wheels"),
            local.join("go-build/00"),
            local.join("go-build/01"),
            local.join("pnpm/store/v10/files"),
            home.join(".gradle/caches/build-cache-1"),
            home.join(".gradle/init.d"),
        ] {
            fs::create_dir_all(path).unwrap();
        }
        let items = discover(&Locations {
            home: home.clone(),
            local_cache: local,
            platform: Platform::Windows,
        });
        assert_eq!(items.len(), 5);
        assert!(items.iter().any(|i| i.id == "npm-cache"));
        assert!(items.iter().any(|i| i.id == "pnpm-v10"));
        let npm = items.iter().find(|i| i.id == "npm-cache").unwrap();
        let npm_path = fs::canonicalize(&npm.path).unwrap();
        let locations = Locations {
            home: home.clone(),
            local_cache: temp.path().canonicalize().unwrap().join("local"),
            platform: Platform::Windows,
        };
        assert!(validates_in(&locations, "npm-cache", &npm_path));
        fs::remove_dir(npm.path.join("index-v5")).unwrap();
        assert!(!validates_in(&locations, "npm-cache", &npm_path));
        assert!(
            items
                .iter()
                .all(|i| !i.path.ends_with(".gradle") && !i.path.ends_with("init.d"))
        );
    }
    #[cfg(unix)]
    #[test]
    fn rejects_linked_cache_and_linked_markers() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap().join("home");
        fs::create_dir_all(home.join(".npm/_cacache/content-v2")).unwrap();
        fs::create_dir_all(home.join("outside")).unwrap();
        symlink(home.join("outside"), home.join(".npm/_cacache/index-v5")).unwrap();
        let items = discover(&Locations {
            home: home.clone(),
            local_cache: home.join("Library/Caches"),
            platform: Platform::Mac,
        });
        assert!(items.iter().all(|i| i.id != "npm-cache"));
    }
    #[test]
    fn scanning_selected_global_caches_counts_bytes_and_rejects_overlaps() {
        use crate::{ScanEvent, scan_global_caches};
        use std::sync::{Arc, Mutex, atomic::AtomicBool};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("home");
        let local = root.join("local");
        for dir in [
            "npm-cache/_cacache/content-v2",
            "npm-cache/_cacache/index-v5",
            "go-build/00",
            "go-build/01",
        ] {
            fs::create_dir_all(local.join(dir)).unwrap();
        }
        fs::write(local.join("npm-cache/_cacache/content-v2/a"), [1u8; 7]).unwrap();
        fs::write(local.join("npm-cache/_cacache/index-v5/b"), [2u8; 5]).unwrap();
        fs::write(local.join("go-build/00/c"), [3u8; 11]).unwrap();
        let locations = Locations {
            home,
            local_cache: local,
            platform: Platform::Windows,
        };
        let caches = discover(&locations);
        assert_eq!(caches.len(), 2);
        let found = Mutex::new(Vec::new());
        let summary = scan_global_caches(
            caches.clone(),
            4,
            Arc::new(AtomicBool::new(false)),
            |event| {
                if let ScanEvent::Sized(item) = event {
                    found.lock().unwrap().push(item);
                }
            },
        );
        let items = found.into_inner().unwrap();
        assert_eq!(
            (
                summary.progress.found,
                summary.progress.files,
                summary.progress.bytes,
                summary.progress.errors
            ),
            (2, 3, 23, 0)
        );
        assert!(
            items
                .iter()
                .all(|item| item.measured && item.complete && item.rule.starts_with("global:"))
        );
        for item in &items {
            assert!(validates_in(
                &locations,
                item.rule.strip_prefix("global:").unwrap(),
                &item.path
            ));
        }
        let mut duplicate = caches;
        duplicate.push(duplicate[0].clone());
        let overlap = scan_global_caches(duplicate, 2, Arc::new(AtomicBool::new(false)), |_| {});
        assert_eq!(
            (
                overlap.progress.found,
                overlap.progress.errors,
                overlap.progress.bytes
            ),
            (2, 1, 23)
        );
    }
}
