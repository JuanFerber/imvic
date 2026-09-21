//! Background file watcher for automatic live-reloading.
//!
//! Monitors target files on disk using a 200ms trailing-edge debounce pipeline
//! to absorb multi-stage editor flushes and atomic renames before triggering reloads.

use crate::input::AppEvent;
use anyhow::{Context, Result};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::time::Duration;

/// Trailing-edge quiet duration required before emitting a reload event.
///
/// Guarantees that multi-stage editor saves (truncate, flush, atomic rename)
/// have completely finished and closed the file before decoding begins.
const DEFAULT_DEBOUNCE_DURATION: Duration = Duration::from_millis(200);

/// Background file system watcher.
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    /// Spawns a background watcher thread on the target file with 200ms trailing-edge debouncing.
    ///
    /// The debouncer waits for a continuous 200ms quiet window after the last detected filesystem
    /// mutation before transmitting an [`AppEvent::FileModified`] signal, ensuring partial saves
    /// and atomic renames have concluded before decoding begins.
    pub fn new(path: &Path, tx: Sender<AppEvent>) -> Result<Self> {
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let watched_file_name = canonical_path.file_name().map(|n| n.to_os_string());

        // Calculate parent directory before moving path into closure
        let watch_target = canonical_path
            .parent()
            .unwrap_or(&canonical_path)
            .to_path_buf();

        let target_path = canonical_path;
        let (raw_tx, raw_rx) = channel();

        // Spawn a dedicated debouncing worker to absorb high-frequency write bursts.
        std::thread::spawn(move || {
            // Block idle until the first filesystem mutation arrives.
            while let Ok(()) = raw_rx.recv() {
                loop {
                    match raw_rx.recv_timeout(DEFAULT_DEBOUNCE_DURATION) {
                        Ok(()) => {
                            // File mutation occurred within the quiet window; reset the timeout.
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            // Quiet window elapsed with zero incoming events; editor has finalized disk flush.
                            let _ = tx.send(AppEvent::FileModified);
                            break;
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            // Watcher channel closed upon drop; terminate background worker cleanly.
                            return;
                        }
                    }
                }
            }
        });

        // Notify callback invoked on inotify/system worker thread.
        // Forwards raw change notifications to the debouncer without holding locks.
        let event_handler = move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let matches_target = event.paths.iter().any(|p| {
                    p == &target_path
                        || p.file_name().map(|n| n.to_os_string()) == watched_file_name
                });

                if matches_target
                    && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                {
                    let _ = raw_tx.send(());
                }
            }
        };

        let mut watcher = RecommendedWatcher::new(event_handler, Config::default())
            .context("Failed to initialize file watcher")?;

        // Watch parent directory to capture atomic renames from editors
        watcher
            .watch(&watch_target, RecursiveMode::NonRecursive)
            .with_context(|| format!("Failed to watch path {:?}", watch_target))?;

        Ok(Self { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::mpsc::channel;

    #[test]
    fn test_file_watcher_creation() {
        let (tx, _rx) = channel();
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_imvic_watch.svg");
        let mut file = std::fs::File::create(&test_file).expect("Create test file");
        let _ = file.write_all(b"<svg></svg>");

        let watcher = FileWatcher::new(&test_file, tx);
        assert!(watcher.is_ok());

        let _ = std::fs::remove_file(&test_file);
    }

    #[test]
    fn test_file_watcher_debounces_burst_events() {
        let (tx, rx) = channel();
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_imvic_debounce.svg");

        // Create initial placeholder file
        let mut file = std::fs::File::create(&test_file).expect("Create test file");
        let _ = file.write_all(b"<svg></svg>");
        drop(file);

        let watcher = FileWatcher::new(&test_file, tx).expect("Create FileWatcher");

        // Allow inotify watcher thread to establish OS event hook
        std::thread::sleep(Duration::from_millis(50));

        // Simulate multi-stage editor save burst: 3 rapid writes spaced by 30ms
        for i in 0..3 {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&test_file)
                .expect("Open test file for write");
            writeln!(f, "<svg><g id='frame_{}'/></svg>", i).expect("Write chunk");
            f.flush().expect("Flush write");
            drop(f);
            std::thread::sleep(Duration::from_millis(30));
        }

        // Wait for the trailing-edge quiet window (200ms) to elapse and trigger reload
        let first_event = rx.recv_timeout(Duration::from_millis(1000));
        assert_eq!(
            first_event,
            Ok(AppEvent::FileModified),
            "Debouncer must emit FileModified after burst quiescence"
        );

        // Verify zero duplicate events: trailing writes must be absorbed into the single event
        let duplicate_event = rx.recv_timeout(Duration::from_millis(150));
        assert!(
            duplicate_event.is_err(),
            "Burst writes within debounce window must not emit redundant reload events"
        );

        let _ = std::fs::remove_file(&test_file);
        drop(watcher);
    }
}
