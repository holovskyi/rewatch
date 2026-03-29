use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

pub enum WatchEvent {
    FileChanged(PathBuf),
    Trigger,
}

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<WatchEvent>,
}

impl FileWatcher {
    pub fn new(
        watch_paths: &[PathBuf],
        extensions: &[String],
        trigger: Option<&Path>,
    ) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();

        let ext_filter: HashSet<String> = extensions.iter().cloned().collect();
        // Cache canonical path; OnceLock allows lazy init if file didn't exist at startup
        let trigger_canonical: OnceLock<PathBuf> = OnceLock::new();
        if let Some(t) = trigger {
            if let Ok(c) = t.canonicalize() {
                let _ = trigger_canonical.set(c);
            }
        }
        let trigger_raw = trigger.map(|t| t.to_path_buf());

        let mut watcher = RecommendedWatcher::new(
            move |result: Result<Event, notify::Error>| {
                let event = match result {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("rewatch: watcher error: {e}");
                        return;
                    }
                };

                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
                    _ => return,
                }

                for path in &event.paths {
                    // Check if this is the trigger file
                    if is_trigger(path, &trigger_canonical, &trigger_raw) {
                        let _ = tx.send(WatchEvent::Trigger);
                        return;
                    }

                    // Filter by extension if configured
                    if !ext_filter.is_empty() {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if !ext_filter.contains(ext) {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }

                    let _ = tx.send(WatchEvent::FileChanged(path.clone()));
                }
            },
            notify::Config::default(),
        )
        .map_err(|e| format!("Failed to create watcher: {e}"))?;

        for path in watch_paths {
            let mode = if path.is_dir() {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            watcher
                .watch(path, mode)
                .map_err(|e| format!("Failed to watch {}: {e}", path.display()))?;
        }

        // Watch trigger file's parent directory
        if let Some(trigger_path) = trigger {
            if let Some(parent) = trigger_path.parent() {
                let parent = if parent.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    parent
                };
                if let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                    eprintln!("rewatch: warning: could not watch trigger directory {}: {e}", parent.display());
                }
            }
        }

        Ok(FileWatcher {
            _watcher: watcher,
            rx,
        })
    }

    /// Try to receive an event without blocking
    pub fn try_recv(&self) -> Option<WatchEvent> {
        self.rx.try_recv().ok()
    }

    /// Drain all pending events, return unique changed paths and whether trigger was hit
    pub fn drain_pending(&self) -> (Vec<PathBuf>, bool) {
        let mut files = HashSet::new();
        let mut triggered = false;

        while let Ok(event) = self.rx.try_recv() {
            match event {
                WatchEvent::FileChanged(p) => {
                    files.insert(p);
                }
                WatchEvent::Trigger => {
                    triggered = true;
                }
            }
        }

        (files.into_iter().collect(), triggered)
    }

    /// Wait a short time to let multiple rapid events settle, then drain
    pub fn debounce_drain(&self, duration: Duration) -> (Vec<PathBuf>, bool) {
        std::thread::sleep(duration);
        self.drain_pending()
    }
}

/// Compare event path against trigger path using canonical paths.
/// Uses OnceLock to cache the first successful canonicalization (trigger may not exist at startup).
fn is_trigger(
    event_path: &Path,
    trigger_canonical: &OnceLock<PathBuf>,
    trigger_raw: &Option<PathBuf>,
) -> bool {
    let trigger_raw = match trigger_raw {
        Some(t) => t,
        None => return false,
    };

    if let Ok(ec) = event_path.canonicalize() {
        // get_or_try_init: use cached value, or try to canonicalize now and cache it
        if let Some(tc) = trigger_canonical.get().or_else(|| {
            trigger_raw.canonicalize().ok().and_then(|c| {
                let _ = trigger_canonical.set(c);
                trigger_canonical.get()
            })
        }) {
            return ec == *tc;
        }
    }

    // Fallback: compare raw paths (canonicalize failed for both)
    event_path == trigger_raw.as_path()
}
