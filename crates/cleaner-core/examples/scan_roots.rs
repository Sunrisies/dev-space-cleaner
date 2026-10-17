//! Read-only CLI for reproducing the same engine independently of the GUI.
use huanj_core::{Kind, ScanEvent, ScanOptions, drives, scan};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};
fn main() {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let roots = if args.is_empty() {
        drives().into_iter().map(|d| d.path).collect()
    } else {
        args
    };
    scan(
        ScanOptions {
            roots,
            kinds: vec![Kind::Node, Kind::Rust, Kind::Python],
            workers: 4,
        },
        Arc::new(AtomicBool::new(false)),
        |event| {
            if matches!(
                event,
                ScanEvent::Sized(_)
                    | ScanEvent::Issue { .. }
                    | ScanEvent::RootStarted { .. }
                    | ScanEvent::RootFinished { .. }
                    | ScanEvent::RootSkipped { .. }
                    | ScanEvent::Finished(_)
            ) {
                println!(
                    "{}",
                    serde_json::to_string(&event).expect("serialize scan result")
                );
            }
        },
    );
}
