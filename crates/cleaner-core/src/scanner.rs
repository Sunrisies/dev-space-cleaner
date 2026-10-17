use crate::platform::{excluded, is_link};
use crate::rules::{Kind, identify};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub id: u64,
    pub path: PathBuf,
    pub root: PathBuf,
    pub project: String,
    pub kind: Kind,
    pub rule: String,
    pub evidence: String,
    pub bytes: u64,
    pub files: u64,
    pub complete: bool,
    pub measured: bool,
    pub skipped_links: u64,
    pub errors: u64,
    pub modified: Option<u64>,
    #[serde(skip)]
    pub(crate) created: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub roots: Vec<PathBuf>,
    pub kinds: Vec<Kind>,
    pub workers: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Progress {
    pub directories: u64,
    pub files: u64,
    pub found: u64,
    pub bytes: u64,
    pub errors: u64,
    pub skipped_links: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub progress: Progress,
    pub cancelled: bool,
    pub elapsed_ms: u64,
    pub roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ScanEvent {
    Found(Candidate),
    Sized(Candidate),
    Progress {
        progress: Progress,
        path: PathBuf,
    },
    RootStarted {
        path: PathBuf,
    },
    RootFinished {
        path: PathBuf,
        errors: u64,
        cancelled: bool,
    },
    RootSkipped {
        path: PathBuf,
        reason: String,
    },
    Issue {
        path: PathBuf,
        message: String,
    },
    Finished(ScanSummary),
}

#[derive(Default)]
struct Counters {
    dirs: AtomicU64,
    files: AtomicU64,
    found: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
    links: AtomicU64,
}
impl Counters {
    fn snapshot(&self) -> Progress {
        Progress {
            directories: self.dirs.load(Ordering::Relaxed),
            files: self.files.load(Ordering::Relaxed),
            found: self.found.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            skipped_links: self.links.load(Ordering::Relaxed),
        }
    }
}

struct Queue {
    pending: usize,
    jobs: VecDeque<DirectoryJob>,
}
struct DirectoryJob {
    path: PathBuf,
    // Directory enumeration already supplies this on Windows. Reusing it avoids
    // a second filesystem query for every discovered directory.
    metadata: Option<fs::Metadata>,
}
type Sink<'a> = dyn Fn(ScanEvent) + Send + Sync + 'a;
struct Engine<'a> {
    cancel: &'a AtomicBool,
    counters: Counters,
    last_tick: Mutex<Instant>,
    sink: &'a Sink<'a>,
    kinds: &'a [Kind],
}

impl Engine<'_> {
    fn progress(&self, path: &Path) {
        if let Ok(mut tick) = self.last_tick.try_lock()
            && tick.elapsed().as_millis() >= 150
        {
            *tick = Instant::now();
            (self.sink)(ScanEvent::Progress {
                progress: self.counters.snapshot(),
                path: path.into(),
            });
        }
    }
    fn issue(&self, path: &Path, err: impl std::fmt::Display) {
        let count = self.counters.errors.fetch_add(1, Ordering::Relaxed);
        // Bound diagnostic traffic during full-drive scans; total remains exact.
        if count < 100 {
            (self.sink)(ScanEvent::Issue {
                path: path.into(),
                message: err.to_string(),
            });
        }
    }
    fn flush_counts(&self, files: &mut u64, bytes: &mut u64, links: &mut u64) {
        self.counters
            .files
            .fetch_add(std::mem::take(files), Ordering::Relaxed);
        self.counters
            .bytes
            .fetch_add(std::mem::take(bytes), Ordering::Relaxed);
        self.counters
            .links
            .fetch_add(std::mem::take(links), Ordering::Relaxed);
    }
    fn measure(&self, item: &mut Candidate) {
        let mut stack = vec![item.path.clone()];
        while let Some(dir) = stack.pop() {
            if self.cancel.load(Ordering::Relaxed) {
                item.complete = false;
                break;
            }
            if dir != item.path {
                self.counters.dirs.fetch_add(1, Ordering::Relaxed);
            }
            let entries = match fs::read_dir(&dir) {
                Ok(e) => e,
                Err(e) => {
                    item.errors += 1;
                    self.issue(&dir, e);
                    continue;
                }
            };
            let (mut batch_files, mut batch_bytes, mut batch_links) = (0, 0, 0);
            for (index, entry) in entries.enumerate() {
                if index % 256 == 0 {
                    self.flush_counts(&mut batch_files, &mut batch_bytes, &mut batch_links);
                    self.progress(&dir);
                }
                if self.cancel.load(Ordering::Relaxed) {
                    item.complete = false;
                    break;
                }
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        item.errors += 1;
                        self.issue(&dir, e);
                        continue;
                    }
                };
                // DirEntry::metadata does not follow symlinks. Windows can reuse
                // enumeration metadata instead of opening every file for a stat.
                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        item.errors += 1;
                        self.issue(&entry.path(), e);
                        continue;
                    }
                };
                if is_link(&meta) {
                    item.skipped_links += 1;
                    batch_links += 1;
                    continue;
                }
                if meta.is_dir() {
                    stack.push(entry.path());
                } else if meta.is_file() {
                    match item.bytes.checked_add(meta.len()) {
                        Some(v) => {
                            item.bytes = v;
                            batch_bytes += meta.len();
                        }
                        None => {
                            item.complete = false;
                            item.errors += 1;
                            self.issue(&dir, "逻辑字节总和超过可表示范围");
                        }
                    }
                    item.files += 1;
                    batch_files += 1;
                }
            }
            self.flush_counts(&mut batch_files, &mut batch_bytes, &mut batch_links);
            self.progress(&dir);
        }
        item.complete &= item.errors == 0 && !self.cancel.load(Ordering::Relaxed);
        item.measured = true;
    }
    fn directory(&self, job: DirectoryJob, root: &Path, children: &mut Vec<DirectoryJob>) {
        let path = &job.path;
        if self.cancel.load(Ordering::Relaxed) {
            return;
        }
        let meta = match job
            .metadata
            .map(Ok)
            .unwrap_or_else(|| fs::symlink_metadata(path))
        {
            Ok(m) => m,
            Err(e) => {
                self.issue(path, e);
                return;
            }
        };
        if is_link(&meta) {
            self.counters.links.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if !meta.is_dir() {
            self.issue(path, "扫描根不是目录");
            return;
        }
        self.counters.dirs.fetch_add(1, Ordering::Relaxed);
        self.progress(path);
        if let Some(recognition) = identify(path) {
            let kind = recognition.kind;
            if self.kinds.contains(&kind) {
                let id = self.counters.found.fetch_add(1, Ordering::Relaxed) + 1;
                let mut item = Candidate {
                    id,
                    path: path.into(),
                    root: root.into(),
                    project: path
                        .parent()
                        .and_then(Path::file_name)
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    kind,
                    rule: recognition.rule.into(),
                    evidence: recognition.evidence.into(),
                    bytes: 0,
                    files: 0,
                    complete: true,
                    measured: false,
                    skipped_links: 0,
                    errors: 0,
                    modified: meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs()),
                    created: meta.created().ok(),
                };
                (self.sink)(ScanEvent::Found(item.clone()));
                self.measure(&mut item);
                (self.sink)(ScanEvent::Sized(item));
            }
            // Never recurse into generated environments, even when their type is disabled.
            return;
        }
        let entries = match fs::read_dir(path) {
            Ok(e) => e,
            Err(e) => {
                self.issue(path, e);
                return;
            }
        };
        for (index, entry) in entries.enumerate() {
            if index % 256 == 0 {
                self.progress(path);
            }
            if self.cancel.load(Ordering::Relaxed) {
                break;
            }
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    self.issue(path, e);
                    continue;
                }
            };
            let ty = match entry.file_type() {
                Ok(t) => t,
                Err(e) => {
                    self.issue(&entry.path(), e);
                    continue;
                }
            };
            if ty.is_symlink() {
                self.counters.links.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            if ty.is_dir() {
                let child = entry.path();
                if excluded(&child) {
                    continue;
                }
                match entry.metadata() {
                    Ok(meta) if is_link(&meta) => {
                        self.counters.links.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(meta) => children.push(DirectoryJob {
                        path: child,
                        metadata: Some(meta),
                    }),
                    Err(e) => self.issue(&child, e),
                }
            }
        }
    }
}

/// Roots are normalized and reduced to disjoint trees. No target contents are read.
pub fn scan(
    options: ScanOptions,
    cancel: Arc<AtomicBool>,
    sink: impl Fn(ScanEvent) + Send + Sync,
) -> ScanSummary {
    let start = Instant::now();
    let engine = Engine {
        cancel: &cancel,
        counters: Counters::default(),
        last_tick: Mutex::new(Instant::now()),
        sink: &sink,
        kinds: &options.kinds,
    };
    let mut seen = HashSet::new();
    let mut roots = Vec::new();
    let mut original_roots = HashMap::new();
    for root in options.roots {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match fs::symlink_metadata(&root) {
            Ok(meta) if is_link(&meta) => {
                engine.issue(&root, "不跟随链接或 reparse point");
                sink(ScanEvent::RootSkipped {
                    path: root,
                    reason: "链接目录已跳过".into(),
                });
                continue;
            }
            Ok(meta) if !meta.is_dir() => {
                engine.issue(&root, "扫描根不是目录");
                sink(ScanEvent::RootSkipped {
                    path: root,
                    reason: "不是目录".into(),
                });
                continue;
            }
            Err(e) => {
                engine.issue(&root, &e);
                sink(ScanEvent::RootSkipped {
                    path: root,
                    reason: e.to_string(),
                });
                continue;
            }
            _ => {}
        }
        match fs::canonicalize(&root) {
            Ok(p) => {
                if seen.insert(p.clone()) {
                    original_roots.insert(p.clone(), root);
                    roots.push(p);
                } else {
                    sink(ScanEvent::RootSkipped {
                        path: root,
                        reason: "重复范围已合并".into(),
                    });
                }
            }
            Err(e) => {
                engine.issue(&root, &e);
                sink(ScanEvent::RootSkipped {
                    path: root,
                    reason: e.to_string(),
                });
            }
        }
    }
    roots.sort_by_key(|p| p.components().count());
    let mut disjoint: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !disjoint.iter().any(|r| root.starts_with(r)) {
            disjoint.push(root);
        } else {
            sink(ScanEvent::RootSkipped {
                path: original_roots[&root].clone(),
                reason: "已包含在其他扫描范围内".into(),
            });
        }
    }
    let workers = options.workers.clamp(1, 16);
    // Roots run in order, avoiding random I/O across slow or external drives.
    // Each root's directory discovery and independent target sizes use the worker pool.
    for root in &disjoint {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let original = &original_roots[root];
        sink(ScanEvent::RootStarted {
            path: original.clone(),
        });
        let errors_before = engine.counters.errors.load(Ordering::Relaxed);
        let queue = (
            Mutex::new(Queue {
                pending: 1,
                jobs: VecDeque::from([DirectoryJob {
                    path: root.clone(),
                    metadata: None,
                }]),
            }),
            Condvar::new(),
        );
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let engine = &engine;
                let queue = &queue;
                scope.spawn(move || {
                    loop {
                        let job = {
                            let mut q = queue.0.lock().unwrap();
                            loop {
                                if engine.cancel.load(Ordering::Relaxed) || q.pending == 0 {
                                    return;
                                }
                                if let Some(job) = q.jobs.pop_front() {
                                    break job;
                                }
                                q = queue
                                    .1
                                    .wait_timeout(q, std::time::Duration::from_millis(100))
                                    .unwrap()
                                    .0;
                            }
                        };
                        let mut local = vec![job];
                        while let Some(job) = local.pop() {
                            if engine.cancel.load(Ordering::Relaxed) {
                                queue.1.notify_all();
                                return;
                            }
                            let mut children = Vec::new();
                            engine.directory(job, root, &mut children);
                            let mut q = queue.0.lock().unwrap();
                            for child in children {
                                if q.jobs.len() < 2048 {
                                    q.pending += 1;
                                    q.jobs.push_back(child);
                                } else {
                                    local.push(child);
                                }
                            }
                            queue.1.notify_all();
                        }
                        let mut q = queue.0.lock().unwrap();
                        q.pending -= 1;
                        queue.1.notify_all();
                    }
                });
            }
        });
        sink(ScanEvent::RootFinished {
            path: original.clone(),
            errors: engine.counters.errors.load(Ordering::Relaxed) - errors_before,
            cancelled: cancel.load(Ordering::Relaxed),
        });
    }
    let summary = ScanSummary {
        progress: engine.counters.snapshot(),
        cancelled: cancel.load(Ordering::Relaxed),
        elapsed_ms: start.elapsed().as_millis() as u64,
        roots: disjoint,
    };
    sink(ScanEvent::Finished(summary.clone()));
    summary
}
