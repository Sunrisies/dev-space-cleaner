use huanj_core::{Candidate, Kind, ScanEvent, ScanOptions, scan};
use std::{
    fs,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

fn collect(
    roots: Vec<std::path::PathBuf>,
    kinds: Vec<Kind>,
    workers: usize,
) -> (Vec<Candidate>, huanj_core::ScanSummary) {
    let items = Mutex::new(Vec::new());
    let summary = scan(
        ScanOptions {
            roots,
            kinds,
            workers,
        },
        Arc::new(AtomicBool::new(false)),
        |e| {
            if let ScanEvent::Sized(item) = e {
                items.lock().unwrap().push(item);
            }
        },
    );
    (items.into_inner().unwrap(), summary)
}
fn fixture(root: &Path) -> u64 {
    fs::create_dir_all(root.join("web/node_modules/pkg/node_modules/nested")).unwrap();
    fs::write(root.join("web/package.json"), "{}").unwrap();
    fs::write(root.join("web/node_modules/pkg/a.js"), [1u8; 713]).unwrap();
    fs::write(
        root.join("web/node_modules/pkg/node_modules/nested/package.json"),
        "{}",
    )
    .unwrap();
    fs::write(
        root.join("web/node_modules/pkg/node_modules/nested/b.js"),
        [0u8; 101],
    )
    .unwrap();
    fs::create_dir_all(root.join("rust/target/debug")).unwrap();
    fs::write(root.join("rust/Cargo.toml"), "[workspace]").unwrap();
    fs::write(root.join("rust/target/debug/bin"), [2u8; 2345]).unwrap();
    fs::create_dir_all(root.join("python/.venv/lib")).unwrap();
    fs::write(root.join("python/.venv/pyvenv.cfg"), "home = python").unwrap();
    fs::write(root.join("python/.venv/lib/pkg.py"), [3u8; 99]).unwrap();
    713 + 2 + 101 + 2345 + 13 + 99
}

#[test]
fn exact_sizes_no_nested_duplicates_and_overlap_deduplication() {
    let root = tempfile::tempdir().unwrap();
    let expected = fixture(root.path());
    for workers in [1, 2, 8] {
        let (items, summary) = collect(
            vec![
                root.path().join("web"),
                root.path().into(),
                root.path().into(),
            ],
            vec![Kind::Node, Kind::Rust, Kind::Python],
            workers,
        );
        assert_eq!(items.len(), 3);
        assert!(items.iter().all(|i| i.measured && i.complete));
        assert_eq!(items.iter().map(|i| i.bytes).sum::<u64>(), expected);
        assert_eq!(summary.progress.bytes, expected);
        assert_eq!(summary.progress.files, 6);
        assert_eq!(summary.progress.errors, 0);
        assert_eq!(summary.roots.len(), 1);
        assert_eq!(items.iter().map(|i| i.files).sum::<u64>(), 6);
    }
}

#[test]
fn does_not_classify_unrelated_same_name_directories() {
    let root = tempfile::tempdir().unwrap();
    for name in ["node_modules", "target", ".venv"] {
        fs::create_dir(root.path().join(name)).unwrap();
    }
    let (items, _) = collect(
        vec![root.path().into()],
        vec![Kind::Node, Kind::Rust, Kind::Python],
        4,
    );
    assert!(items.is_empty());
}

#[test]
fn multiple_roots_survive_a_missing_drive() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let a = fixture(first.path());
    let b = fixture(second.path());
    let (items, summary) = collect(
        vec![
            first.path().into(),
            first.path().join("not-mounted"),
            second.path().into(),
        ],
        vec![Kind::Node, Kind::Rust, Kind::Python],
        4,
    );
    assert_eq!(items.len(), 6);
    assert_eq!(summary.progress.bytes, a + b);
    assert_eq!(summary.progress.errors, 1);
    assert_eq!(summary.roots.len(), 2);
}

#[test]
fn language_filter_and_target_root() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let (items, _) = collect(vec![root.path().into()], vec![Kind::Rust], 2);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, Kind::Rust);
    let (items, _) = collect(
        vec![root.path().join("python/.venv")],
        vec![Kind::Python],
        1,
    );
    assert_eq!(items.len(), 1);
}

#[test]
fn cancellation_during_measurement_is_incomplete() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let cancel = Arc::new(AtomicBool::new(false));
    let items = Mutex::new(Vec::new());
    let summary = scan(
        ScanOptions {
            roots: vec![root.path().into()],
            kinds: vec![Kind::Node],
            workers: 1,
        },
        cancel.clone(),
        |e| match e {
            ScanEvent::Found(_) => cancel.store(true, Ordering::Relaxed),
            ScanEvent::Sized(item) => items.lock().unwrap().push(item),
            _ => {}
        },
    );
    assert!(summary.cancelled);
    assert!(!items.lock().unwrap()[0].complete);
}

#[test]
fn queue_pressure_completes_without_losing_targets() {
    let root = tempfile::tempdir().unwrap();
    for n in 0..2100 {
        fs::create_dir(root.path().join(format!("d{n}"))).unwrap();
    }
    fixture(&root.path().join("d2099"));
    let (items, summary) = collect(
        vec![root.path().into()],
        vec![Kind::Node, Kind::Rust, Kind::Python],
        1,
    );
    assert_eq!(items.len(), 3);
    assert_eq!(summary.progress.errors, 0);
}

#[test]
fn batches_preserve_every_byte_and_trailing_file() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("中文项目/node_modules");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.parent().unwrap().join("package.json"), "{}").unwrap();
    let mut expected = 0;
    for n in 0..769 {
        let length = n % 37;
        fs::write(target.join(format!("文件-{n}.js")), vec![0; length]).unwrap();
        expected += length as u64;
    }
    for workers in [1, 4] {
        let (items, summary) = collect(vec![root.path().into()], vec![Kind::Node], workers);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].files, 769);
        assert_eq!(items[0].bytes, expected);
        assert_eq!(summary.progress.files, 769);
        assert_eq!(summary.progress.bytes, expected);
        assert!(items[0].complete);
    }
}

#[cfg(unix)]
#[test]
fn links_are_not_followed_or_counted() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fixture(root.path());
    fs::write(outside.path().join("large"), [0u8; 10000]).unwrap();
    std::os::unix::fs::symlink(
        outside.path(),
        root.path().join("web/node_modules/external"),
    )
    .unwrap();
    std::os::unix::fs::symlink(root.path(), root.path().join("cycle")).unwrap();
    let (items, summary) = collect(
        vec![root.path().into()],
        vec![Kind::Node, Kind::Rust, Kind::Python],
        4,
    );
    assert_eq!(items.len(), 3);
    assert_eq!(summary.progress.bytes, 3273);
    assert_eq!(summary.progress.skipped_links, 2);
}

#[cfg(windows)]
#[test]
fn windows_directory_link_is_not_followed() {
    use std::os::windows::fs::symlink_dir;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fixture(root.path());
    // Developer Mode is not guaranteed on all Windows runners.
    if let Err(e) = symlink_dir(outside.path(), root.path().join("external")) {
        eprintln!("SKIP Windows symlink creation requires Developer Mode or privilege: {e}");
        return;
    }
    let (_, summary) = collect(vec![root.path().into()], vec![Kind::Node], 4);
    assert_eq!(summary.progress.skipped_links, 1);
}
