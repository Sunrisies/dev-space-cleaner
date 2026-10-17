use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize)]
pub struct Drive {
    pub path: PathBuf,
    pub label: String,
}

pub fn is_link(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn drives() -> Vec<Drive> {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetLogicalDrives() -> u32;
            fn GetDriveTypeW(root: *const u16) -> u32;
        }
        // These calls inspect the OS drive map without traversing the drive.
        let mask = unsafe { GetLogicalDrives() };
        (0..26)
            .filter(|i| mask & (1 << i) != 0)
            .filter_map(|i| {
                let path = format!("{}:\\", (b'A' + i) as char);
                let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
                let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
                if !matches!(kind, 2..=6) {
                    return None;
                }
                let label = match kind {
                    2 => "可移动磁盘",
                    3 => "本地磁盘",
                    4 => "映射网络盘",
                    5 => "光驱",
                    _ => "磁盘",
                };
                Some(Drive {
                    label: format!("{path} · {label}"),
                    path: path.into(),
                })
            })
            .collect()
    }
    #[cfg(target_os = "macos")]
    {
        let data = PathBuf::from("/System/Volumes/Data");
        let root = if data.is_dir() {
            data
        } else {
            PathBuf::from("/")
        };
        let mut result = vec![Drive {
            label: "Macintosh · 数据卷".into(),
            path: root,
        }];
        if let Ok(entries) = fs::read_dir("/Volumes") {
            for entry in entries.flatten() {
                if let Ok(meta) = fs::symlink_metadata(entry.path()) {
                    if meta.is_dir() && !is_link(&meta) {
                        result.push(Drive {
                            label: entry.file_name().to_string_lossy().into_owned(),
                            path: entry.path(),
                        });
                    }
                }
            }
        }
        result
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        vec![Drive {
            label: "文件系统".into(),
            path: PathBuf::from("/"),
        }]
    }
}

pub(crate) fn excluded(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    if matches!(
        name.as_str(),
        ".git" | ".trash" | ".trashes" | "$recycle.bin" | "system volume information"
    ) {
        return true;
    }
    #[cfg(windows)]
    if path.parent().is_some_and(|p| p.parent().is_none())
        && matches!(
            name.as_str(),
            "windows" | "program files" | "program files (x86)" | "programdata" | "recovery"
        )
    {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        if matches!(
            path.to_str(),
            Some("/System" | "/Library" | "/Applications" | "/private" | "/dev" | "/Volumes")
        ) {
            return true;
        }
        if path.parent() == Some(Path::new("/System/Volumes/Data"))
            && matches!(
                name.as_str(),
                "system" | "library" | "applications" | "private" | "volumes"
            )
        {
            return true;
        }
        if matches!(
            path.extension().and_then(|x| x.to_str()),
            Some("app" | "bundle" | "framework" | "photoslibrary")
        ) {
            return true;
        }
    }
    false
}
