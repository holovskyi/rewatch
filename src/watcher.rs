use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
}

impl std::fmt::Display for ChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeKind::Created => write!(f, "+"),
            ChangeKind::Modified => write!(f, "~"),
            ChangeKind::Removed => write!(f, "-"),
        }
    }
}

pub enum WatchEvent {
    FileChanged(PathBuf, ChangeKind),
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
        cwd: Option<&Path>,
    ) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();

        let ext_filter: HashSet<String> = extensions.iter().cloned().collect();

        let explicit_files = collect_explicit_files(watch_paths, cwd);

        // Cache canonical path; OnceLock allows lazy init if file didn't exist at startup
        let trigger_canonical: OnceLock<PathBuf> = OnceLock::new();
        if let Some(t) = trigger {
            if let Ok(c) = t.canonicalize() {
                let _ = trigger_canonical.set(c);
            }
        }
        let trigger_raw = trigger.map(|t| resolve_abs(t, cwd));

        let mut watcher = RecommendedWatcher::new(
            move |result: Result<Event, notify::Error>| {
                let event = match result {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("rewatch: watcher error: {e}");
                        return;
                    }
                };

                let kind = match event.kind {
                    EventKind::Create(_) => ChangeKind::Created,
                    EventKind::Modify(_) => ChangeKind::Modified,
                    EventKind::Remove(_) => ChangeKind::Removed,
                    _ => return,
                };

                for path in &event.paths {
                    if is_trigger(path, &trigger_canonical, &trigger_raw) {
                        let _ = tx.send(WatchEvent::Trigger);
                        return;
                    }

                    // Fast path: ext filter passes (or is empty) — accept immediately.
                    // Slow path: only when ext would reject, check if this is an
                    // explicitly watched file. canonicalize is a syscall, so we
                    // first try a raw lookup and only canonicalize on miss.
                    let ext_ok = ext_filter.is_empty()
                        || path
                            .extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| ext_filter.contains(e));

                    if !ext_ok {
                        let is_explicit = explicit_files.contains(path)
                            || path
                                .canonicalize()
                                .ok()
                                .is_some_and(|c| explicit_files.contains(&c));
                        if !is_explicit {
                            continue;
                        }
                    }

                    let _ = tx.send(WatchEvent::FileChanged(path.clone(), kind));
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

    /// Drain all pending events, return changed files with kind and whether trigger was hit
    pub fn drain_pending(&self) -> (Vec<(PathBuf, ChangeKind)>, bool) {
        let mut files = HashMap::new();
        let mut triggered = false;

        while let Ok(event) = self.rx.try_recv() {
            match event {
                WatchEvent::FileChanged(p, kind) => {
                    files.insert(p, kind);
                }
                WatchEvent::Trigger => {
                    triggered = true;
                }
            }
        }

        (files.into_iter().collect(), triggered)
    }

    /// Wait a short time to let multiple rapid events settle, then drain
    pub fn debounce_drain(&self, duration: Duration) -> (Vec<(PathBuf, ChangeKind)>, bool) {
        std::thread::sleep(duration);
        self.drain_pending()
    }
}

/// Resolve a path to absolute form: use as-is if already absolute, else join with cwd.
/// Falls back to the original path when no cwd is available.
fn resolve_abs(path: &Path, cwd: Option<&Path>) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(cwd) = cwd {
        cwd.join(path)
    } else {
        path.to_path_buf()
    }
}

/// Collect explicit (non-directory) watch paths in both absolute and canonical forms.
///
/// Both forms are stored so that:
/// - event paths match regardless of which form `notify` delivers (raw vs canonical),
/// - Remove events still match via the absolute form when canonicalize fails.
fn collect_explicit_files(watch_paths: &[PathBuf], cwd: Option<&Path>) -> HashSet<PathBuf> {
    let mut set = HashSet::new();
    for p in watch_paths.iter().filter(|p| !p.is_dir()) {
        let abs = resolve_abs(p, cwd);
        if let Ok(c) = abs.canonicalize() {
            set.insert(c);
        }
        set.insert(abs);
    }
    set
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_trigger_configured() {
        let canonical = OnceLock::new();
        assert!(!is_trigger(Path::new("/any/path"), &canonical, &None));
    }

    #[test]
    fn fallback_matches_absolute_paths() {
        let canonical = OnceLock::new();
        let trigger_raw = Some(PathBuf::from("/project/.rewatch-trigger"));
        assert!(is_trigger(
            Path::new("/project/.rewatch-trigger"),
            &canonical,
            &trigger_raw
        ));
    }

    #[test]
    fn fallback_rejects_relative_vs_absolute() {
        let canonical = OnceLock::new();
        // Relative trigger_raw should NOT match absolute event path
        let trigger_raw = Some(PathBuf::from(".rewatch-trigger"));
        assert!(!is_trigger(
            Path::new("/project/.rewatch-trigger"),
            &canonical,
            &trigger_raw
        ));
    }

    #[test]
    fn fallback_rejects_different_paths() {
        let canonical = OnceLock::new();
        let trigger_raw = Some(PathBuf::from("/project/.rewatch-trigger"));
        assert!(!is_trigger(
            Path::new("/project/src/main.rs"),
            &canonical,
            &trigger_raw
        ));
    }

    #[test]
    fn resolve_abs_keeps_absolute_unchanged() {
        let abs = if cfg!(windows) {
            PathBuf::from("C:\\abs\\path")
        } else {
            PathBuf::from("/abs/path")
        };
        assert_eq!(resolve_abs(&abs, Some(Path::new("/cwd"))), abs);
    }

    #[test]
    fn resolve_abs_joins_relative_with_cwd() {
        let cwd = Path::new("/cwd");
        assert_eq!(resolve_abs(Path::new("file"), Some(cwd)), cwd.join("file"));
    }

    #[test]
    fn resolve_abs_returns_relative_unchanged_without_cwd() {
        assert_eq!(resolve_abs(Path::new("file"), None), PathBuf::from("file"));
    }

    #[test]
    fn explicit_files_skips_directories() {
        // "." is always a directory
        let set = collect_explicit_files(&[PathBuf::from(".")], None);
        assert!(set.is_empty());
    }

    #[test]
    fn explicit_files_includes_nonexistent_as_absolute() {
        let cwd = Path::new("/proj");
        let set = collect_explicit_files(&[PathBuf::from(".env")], Some(cwd));
        // canonicalize fails for nonexistent file, but absolute form must still be present
        assert!(set.contains(&cwd.join(".env")));
    }

    #[test]
    fn explicit_files_includes_existing_in_canonical_form() {
        // Cargo.toml exists in the crate root during tests
        let set = collect_explicit_files(&[PathBuf::from("Cargo.toml")], None);
        let canonical = Path::new("Cargo.toml").canonicalize().unwrap();
        assert!(set.contains(&canonical));
    }
}
