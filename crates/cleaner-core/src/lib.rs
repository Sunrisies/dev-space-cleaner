mod cleaner;
mod platform;
mod scanner;
#[cfg(windows)]
mod windows_trash;

pub use cleaner::{CleanResult, clean};
pub use platform::{Drive, drives, is_link};
pub use scanner::{Candidate, Kind, Progress, ScanEvent, ScanOptions, ScanSummary, scan};
