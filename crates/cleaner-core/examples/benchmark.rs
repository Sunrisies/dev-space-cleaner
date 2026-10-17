//! Reproducible, read-only scanning benchmark. Fixture creation is outside timing.
use huanj_core::{Kind, ScanOptions, scan};
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
};
fn main() {
    let temp = tempfile::tempdir().expect("create dedicated benchmark temp directory");
    let payload = vec![b'x'; 512];
    let mut expected_bytes = 0u64;
    let mut expected_files = 0u64;
    for project in 0..120 {
        let base = temp.path().join(format!("project-{project}"));
        let (folder, marker) = match project % 3 {
            0 => ("node_modules", "package.json"),
            1 => ("target", "Cargo.toml"),
            _ => (".venv", ".venv/pyvenv.cfg"),
        };
        fs::create_dir_all(base.join(folder)).unwrap();
        fs::write(base.join(marker), "{}").unwrap();
        if project % 3 == 2 {
            expected_bytes += 2;
            expected_files += 1;
        }
        for package in 0..20 {
            let dir = base.join(folder).join(format!("pkg-{package}"));
            fs::create_dir_all(&dir).unwrap();
            for file in 0..10 {
                fs::write(dir.join(format!("file-{file}")), &payload).unwrap();
                expected_bytes += payload.len() as u64;
                expected_files += 1;
            }
        }
    }
    println!(
        "Fixture: 120 projects, {expected_files} target files, {expected_bytes} logical bytes"
    );
    for workers in [1, 2, 4, 8] {
        let summary = scan(
            ScanOptions {
                roots: vec![temp.path().into()],
                kinds: vec![Kind::Node, Kind::Rust, Kind::Python],
                workers,
            },
            Arc::new(AtomicBool::new(false)),
            |_| {},
        );
        assert_eq!(summary.progress.bytes, expected_bytes);
        assert_eq!(summary.progress.files, expected_files);
        assert_eq!(summary.progress.found, 120);
        assert_eq!(summary.progress.errors, 0);
        println!(
            "workers={workers}: {} ms, {} files/s, verified exact totals",
            summary.elapsed_ms,
            expected_files * 1000 / summary.elapsed_ms.max(1)
        );
    }
    println!(
        "Repeated scans use OS caches. These numbers do not predict cold full-drive performance."
    );
}
