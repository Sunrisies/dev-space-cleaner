//! Read-only CLI for reproducing the same engine independently of the GUI.
use huanj_core::{Kind, ScanEvent, ScanOptions, drives, scan};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};
fn main() {
    let mut args = Vec::new();
    let mut workers = 8;
    let mut kinds = Kind::all();
    let mut input = std::env::args_os().skip(1);
    while let Some(arg) = input.next() {
        if arg == "--legacy-types" {
            kinds = vec![Kind::Node, Kind::Rust, Kind::Python];
        } else if arg == "--workers" {
            workers = input
                .next()
                .expect("--workers needs a value")
                .to_str()
                .expect("worker count must be UTF-8")
                .parse()
                .expect("worker count must be an integer");
            assert!(
                (1..=16).contains(&workers),
                "worker count must be between 1 and 16"
            );
        } else {
            args.push(PathBuf::from(arg));
        }
    }
    let roots = if args.is_empty() {
        drives().into_iter().map(|d| d.path).collect()
    } else {
        args
    };
    scan(
        ScanOptions {
            roots,
            kinds,
            workers,
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
