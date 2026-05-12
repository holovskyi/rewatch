mod config;
mod process;
mod watcher;

use config::Config;
use process::ManagedChild;
use watcher::{ChangeKind, DrainResult, FileWatcher, WatchEvent};

use std::collections::HashSet;
use std::io::{self, BufRead, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);
static CWD: OnceLock<PathBuf> = OnceLock::new();

const TRIGGER_MSG: &str = "=== Trigger detected, auto-restarting... ===";
const CONFIG_RELOAD_MSG: &str = "=== Config changed, reloading... ===";
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const DEBOUNCE_DURATION: Duration = Duration::from_millis(100);

enum LoopEvent {
    FileChanged(PathBuf, ChangeKind),
    Trigger,
    ConfigChanged,
    ProcessExited(std::process::ExitStatus),
    ProcessError(io::Error),
    CtrlC,
}

enum WaitOutcome {
    /// Enter pressed or a non-config event observed — caller resumes the supervise loop.
    Restart,
    /// ConfigChanged observed while waiting — caller must reload.
    Reload,
    /// SHOULD_EXIT was set — caller must break out.
    Exit,
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

    // Parse CLI / load config FIRST so --help and arg errors print before
    // any of our own startup noise (e.g. the stdin-not-a-terminal warning).
    let initial_config = match Config::load() {
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

    // Remove stale trigger ONCE at startup. Re-doing this on every reload would
    // race with a user who has just touched the trigger to force a restart.
    if let Some(ref t) = initial_config.trigger {
        let _ = std::fs::remove_file(t);
    }
    let mut next_config: Option<Config> = Some(initial_config);

    'reload: loop {
        if should_exit() {
            break;
        }

        // First iteration uses initial_config; subsequent iterations re-load.
        let config = match next_config.take() {
            Some(c) => c,
            None => match Config::load() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            },
        };

        warn_if_trigger_equals_config(&config);

        let watch_list: Vec<_> = config.watch.iter().map(|p| p.display().to_string()).collect();
        println!("rewatch");
        if let Some(ref cp) = config.config_path {
            println!("  config:  {}", cp.display());
        }
        println!("  watch:   {}", watch_list.join(", "));
        if !config.ext.is_empty() {
            println!("  ext:     {}", config.ext.join(", "));
        }
        if let Some(ref t) = config.trigger {
            println!("  trigger: {}", t.display());
        }
        println!("  cmd:     {}", config.command.join(" "));
        println!();

        let file_watcher = match FileWatcher::new(
            &config.watch,
            &config.ext,
            config.trigger.as_deref(),
            config.config_path.as_deref(),
            CWD.get().map(|p| p.as_path()),
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        };

        'supervise: loop {
            if should_exit() {
                break 'reload;
            }

            println!("=== Starting: {} ===", config.command.join(" "));
            println!();

            let mut child = match ManagedChild::spawn(&config.command, &config.env) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to start command: {e}");
                    match prompt_and_wait(&file_watcher, &stdin_rx) {
                        WaitOutcome::Reload => continue 'reload,
                        WaitOutcome::Restart => continue 'supervise,
                        WaitOutcome::Exit => break 'reload,
                    }
                }
            };

            let event = wait_for_event(&file_watcher, &mut child, config.trigger_always);

            match event {
                LoopEvent::ConfigChanged => {
                    println!();
                    println!("{CONFIG_RELOAD_MSG}");
                    child.kill_and_wait();
                    let _ = file_watcher.debounce_drain(DEBOUNCE_DURATION);
                    println!();
                    continue 'reload;
                }
                LoopEvent::FileChanged(path, kind) => {
                    println!();
                    println!("=== Changes detected ===");
                    let mut seen = HashSet::new();
                    seen.insert(path.clone());
                    print_change(&path, kind);
                    child.kill_and_wait();

                    let drain = file_watcher.debounce_drain(DEBOUNCE_DURATION);
                    print_changes_deduped(&mut seen, &drain.files);

                    if drain.config_changed {
                        println!("{CONFIG_RELOAD_MSG}");
                        println!();
                        continue 'reload;
                    }

                    if drain.triggered {
                        println!("{TRIGGER_MSG}");
                        println!();
                        continue 'supervise;
                    }

                    match prompt_and_wait(&file_watcher, &stdin_rx) {
                        WaitOutcome::Reload => continue 'reload,
                        WaitOutcome::Restart => continue 'supervise,
                        WaitOutcome::Exit => break 'reload,
                    }
                }
                LoopEvent::Trigger => {
                    println!();
                    println!("{TRIGGER_MSG}");
                    child.kill_and_wait();
                    let drain = file_watcher.debounce_drain(DEBOUNCE_DURATION);
                    if drain.config_changed {
                        println!("{CONFIG_RELOAD_MSG}");
                        println!();
                        continue 'reload;
                    }
                    println!();
                    continue 'supervise;
                }
                LoopEvent::ProcessExited(status) => {
                    println!();
                    if status.success() {
                        println!("=== Process exited successfully ===");
                    } else {
                        println!("=== Process exited with: {} ===", status);
                    }
                    match prompt_and_wait(&file_watcher, &stdin_rx) {
                        WaitOutcome::Reload => continue 'reload,
                        WaitOutcome::Restart => continue 'supervise,
                        WaitOutcome::Exit => break 'reload,
                    }
                }
                LoopEvent::ProcessError(e) => {
                    println!();
                    println!("=== Process error: {} ===", e);
                    match prompt_and_wait(&file_watcher, &stdin_rx) {
                        WaitOutcome::Reload => continue 'reload,
                        WaitOutcome::Restart => continue 'supervise,
                        WaitOutcome::Exit => break 'reload,
                    }
                }
                LoopEvent::CtrlC => {
                    println!();
                    println!("=== Interrupted ===");
                    SHOULD_EXIT.store(false, Ordering::SeqCst);
                    child.kill_and_wait();
                    match prompt_and_wait(&file_watcher, &stdin_rx) {
                        WaitOutcome::Reload => continue 'reload,
                        WaitOutcome::Restart => continue 'supervise,
                        WaitOutcome::Exit => break 'reload,
                    }
                }
            }
        }
    }

    println!("rewatch: shutting down.");
}

fn warn_if_trigger_equals_config(config: &Config) {
    let (Some(trigger), Some(config_path)) = (config.trigger.as_ref(), config.config_path.as_ref()) else {
        return;
    };
    let config_canonical = config_path.canonicalize().ok();
    let trigger_canonical = trigger.canonicalize().ok();
    let same = match (config_canonical, trigger_canonical) {
        (Some(a), Some(b)) => a == b,
        _ => config_path == trigger,
    };
    if same {
        eprintln!(
            "rewatch: warning: trigger path ({}) equals config path — config reload will fire, trigger won't",
            trigger.display()
        );
    }
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
fn wait_for_event(watcher: &FileWatcher, child: &mut ManagedChild, trigger_always: bool) -> LoopEvent {
    loop {
        if should_exit() {
            return LoopEvent::CtrlC;
        }

        if let Some(event) = watcher.try_recv() {
            match event {
                WatchEvent::FileChanged(p, k) => return LoopEvent::FileChanged(p, k),
                WatchEvent::ConfigChanged => return LoopEvent::ConfigChanged,
                WatchEvent::Trigger => {
                    if trigger_always {
                        return LoopEvent::Trigger;
                    }
                    // Ignore trigger while process is running (default).
                    // Safe: if the agent also changed files, FileChanged will fire
                    // and debounce_drain will pick up the trigger.
                }
            }
        }

        match child.try_wait() {
            Ok(Some(status)) => return LoopEvent::ProcessExited(status),
            Err(e) => return LoopEvent::ProcessError(e),
            Ok(None) => {}
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Print "Press Enter to restart..." and wait for Enter, trigger, or config change.
fn prompt_and_wait(watcher: &FileWatcher, stdin_rx: &mpsc::Receiver<()>) -> WaitOutcome {
    println!();
    println!("Press Enter to restart...");
    let mut seen = HashSet::new();

    loop {
        if should_exit() {
            return WaitOutcome::Exit;
        }

        if stdin_rx.try_recv().is_ok() {
            let DrainResult { files, config_changed, .. } = watcher.drain_pending();
            if config_changed {
                println!("{CONFIG_RELOAD_MSG}");
                return WaitOutcome::Reload;
            }
            let new_files: Vec<_> = files
                .into_iter()
                .filter(|(f, _)| !seen.contains(f))
                .collect();
            if !new_files.is_empty() {
                println!("(accumulated changes while waiting:)");
                print_changes_deduped(&mut seen, &new_files);
            }
            return WaitOutcome::Restart;
        }

        loop {
            match watcher.try_recv() {
                Some(WatchEvent::ConfigChanged) => {
                    println!("{CONFIG_RELOAD_MSG}");
                    return WaitOutcome::Reload;
                }
                Some(WatchEvent::Trigger) => {
                    println!("{TRIGGER_MSG}");
                    return WaitOutcome::Restart;
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
