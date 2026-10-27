mod cleaner;
pub mod global_cache;
mod platform;
mod rules;
mod scanner;
#[cfg(windows)]
mod windows_trash;

pub use cleaner::{CleanResult, clean, validate as validate_candidate};
pub use platform::{Drive, drives, is_link};
pub use rules::Kind;
pub use scanner::{
    Candidate, Progress, ScanEvent, ScanOptions, ScanSummary, scan, scan_global_caches,
};
