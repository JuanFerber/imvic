//! Background file watcher for automatic live-reloading.
//!
//! Monitors target files on disk with a 200ms debounce filter
//! to handle partial editor saves without crashing.

use crate::input::AppEvent;
use anyhow::{Context, Result};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Background file system watcher.
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    /// Spawns a background watcher thread on the target file with 200ms debounce.
    pub fn new(path: &Path, tx: Sender<AppEvent>) -> Result<Self> {
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let watched_file_name = canonical_path.file_name().map(|n| n.to_os_string());

        // Calculate parent directory before moving path into closure
        let watch_target = canonical_path
            .parent()
            .unwrap_or(&canonical_path)
            .to_path_buf();

        let target_path = canonical_path;
        let last_event_time = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));
        let debounce_duration = Duration::from_millis(200);

        let event_handler = move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let matches_target = event.paths.iter().any(|p| {
                    p == &target_path
                        || p.file_name().map(|n| n.to_os_string()) == watched_file_name
                });

                if matches_target
                    && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                    && let Ok(mut last_time) = last_event_time.lock()
                {
                    let now = Instant::now();
                    if now.duration_since(*last_time) >= debounce_duration {
                        *last_time = now;
                        let _ = tx.send(AppEvent::FileModified);
                    }
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
}
