mod config;
mod process;
mod watcher;

use config::Config;
use process::ManagedChild;
use watcher::{ChangeKind, FileWatcher, WatchEvent};

use std::collections::HashSet;
use std::io::{self, BufRead, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);
static CWD: OnceLock<PathBuf> = OnceLock::new();

const TRIGGER_MSG: &str = "=== Trigger detected, auto-restarting... ===";
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const DEBOUNCE_DURATION: Duration = Duration::from_millis(100);

enum LoopEvent {
    FileChanged(PathBuf, ChangeKind),
    Trigger,
    ProcessExited(std::process::ExitStatus),
    ProcessError(io::Error),
    CtrlC,
}

/// Convert absolute path to relative (from cwd). Falls back to original if stripping fails.
fn relative(path: &Path) -> &Path {
    CWD.get()
        .and_then(|cwd| path.strip_prefix(cwd).ok())
        .unwrap_or(path)
}

fn print_change(path: &Path, kind: ChangeKind) {
    println!("  {kind}: {}", relative(path).display());
}

/// Print only files not yet seen; update kind to latest for already-seen files.
fn print_changes_deduped(seen: &mut HashSet<PathBuf>, files: &[(PathBuf, ChangeKind)]) {
    for (f, k) in files {
        if seen.insert(f.clone()) {
            print_change(f, *k);
        }
    }
}

fn should_exit() -> bool {
    SHOULD_EXIT.load(Ordering::SeqCst)
}

fn main() {
    // Cache cwd once at startup
    if let Ok(cwd) = std::env::current_dir() {
        let _ = CWD.set(cwd);
    }

    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    ctrlc::set_handler(move || {
        SHOULD_EXIT.store(true, Ordering::SeqCst);
    })
    .expect("Failed to set Ctrl+C handler");

    let stdin_rx = spawn_stdin_reader();

    let watch_list: Vec<_> = config.watch.iter().map(|p| p.display().to_string()).collect();
    println!("rewatch");
    println!("  watch:   {}", watch_list.join(", "));
    if !config.ext.is_empty() {
        println!("  ext:     {}", config.ext.join(", "));
    }
    if let Some(ref t) = config.trigger {
        println!("  trigger: {}", t.display());
    }
    println!("  cmd:     {}", config.command.join(" "));
    println!();

    // Remove stale trigger file so it doesn't fire unexpectedly
    if let Some(ref t) = config.trigger {
        let _ = std::fs::remove_file(t);
    }

    let file_watcher = match FileWatcher::new(&config.watch, &config.ext, config.trigger.as_deref())
    {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    loop {
        if should_exit() {
            break;
        }

        println!("=== Starting: {} ===", config.command.join(" "));
        println!();

        let mut child = match ManagedChild::spawn(&config.command, &config.env) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to start command: {e}");
                prompt_and_wait(&file_watcher, &stdin_rx);
                continue;
            }
        };

        let event = wait_for_event(&file_watcher, &mut child);

        match event {
            LoopEvent::FileChanged(path, kind) => {
                println!();
                println!("=== Changes detected ===");
                let mut seen = HashSet::new();
                seen.insert(path.clone());
                print_change(&path, kind);
                child.kill_and_wait();

                let (more_files, triggered) =
                    file_watcher.debounce_drain(DEBOUNCE_DURATION);
                print_changes_deduped(&mut seen, &more_files);

                if triggered {
                    println!("{TRIGGER_MSG}");
                    println!();
                    continue;
                }

                prompt_and_wait(&file_watcher, &stdin_rx);
            }
            LoopEvent::Trigger => {
                println!();
                println!("{TRIGGER_MSG}");
                child.kill_and_wait();
                let _ = file_watcher.debounce_drain(DEBOUNCE_DURATION);
                println!();
                continue;
            }
            LoopEvent::ProcessExited(status) => {
                println!();
                if status.success() {
                    println!("=== Process exited successfully ===");
                } else {
                    println!("=== Process exited with: {} ===", status);
                }
                prompt_and_wait(&file_watcher, &stdin_rx);
            }
            LoopEvent::ProcessError(e) => {
                println!();
                println!("=== Process error: {} ===", e);
                prompt_and_wait(&file_watcher, &stdin_rx);
            }
            LoopEvent::CtrlC => {
                child.kill_and_wait();
                break;
            }
        }
    }

    println!("rewatch: shutting down.");
}

/// Spawn a single stdin reader thread that lives forever.
/// If stdin is not a terminal (piped/redirected), warns the user.
fn spawn_stdin_reader() -> mpsc::Receiver<()> {
    if !io::stdin().is_terminal() {
        eprintln!("rewatch: warning: stdin is not a terminal, Enter key won't work (use trigger file or Ctrl+C)");
    }

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if tx.send(()).is_err() {
                break;
            }
        }
    });
    rx
}

/// Wait for either a file event, process exit, or Ctrl+C
fn wait_for_event(watcher: &FileWatcher, child: &mut ManagedChild) -> LoopEvent {
    loop {
        if should_exit() {
            return LoopEvent::CtrlC;
        }

        if let Some(event) = watcher.try_recv() {
            return match event {
                WatchEvent::FileChanged(p, k) => LoopEvent::FileChanged(p, k),
                WatchEvent::Trigger => LoopEvent::Trigger,
            };
        }

        match child.try_wait() {
            Ok(Some(status)) => return LoopEvent::ProcessExited(status),
            Err(e) => return LoopEvent::ProcessError(e),
            Ok(None) => {}
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Print "Press Enter to restart..." and wait for Enter or trigger.
fn prompt_and_wait(watcher: &FileWatcher, stdin_rx: &mpsc::Receiver<()>) {
    println!();
    println!("Press Enter to restart...");
    let mut seen = HashSet::new();

    loop {
        if should_exit() {
            return;
        }

        if stdin_rx.try_recv().is_ok() {
            let (files, _) = watcher.drain_pending();
            let new_files: Vec<_> = files
                .into_iter()
                .filter(|(f, _)| !seen.contains(f))
                .collect();
            if !new_files.is_empty() {
                println!("(accumulated changes while waiting:)");
                print_changes_deduped(&mut seen, &new_files);
            }
            return;
        }

        loop {
            match watcher.try_recv() {
                Some(WatchEvent::Trigger) => {
                    println!("{TRIGGER_MSG}");
                    return;
                }
                Some(WatchEvent::FileChanged(p, k)) => {
                    if seen.insert(p.clone()) {
                        print_change(&p, k);
                    }
                }
                None => break,
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}
