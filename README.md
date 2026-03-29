# rewatch

Cross-platform file watcher that restarts commands on changes. Event-driven (no polling), kills entire process tree, waits for your confirmation before restarting.

## Install

```bash
cargo install --path .
```

## Usage

### With CLI arguments

```bash
rewatch -w src,Cargo.toml -e rs,toml -- cargo run
```

### With config file

Create `rewatch.toml` in your project root:

```toml
command = "cargo run"
watch = ["src", "Cargo.toml"]
ext = ["rs", "toml"]

[env]
RUST_LOG = "debug"
```

Then just run:

```bash
rewatch
```

CLI arguments override config file values.

## How it works

1. Starts your command
2. Watches specified files/directories for changes
3. On change — kills the process (entire tree) and shows what changed
4. Waits for **Enter** before restarting (so you can finish your edits)
5. On process crash — shows exit code, waits for Enter

## CLI options

| Option | Description |
|---|---|
| `-w, --watch <paths>` | Paths to watch, comma-separated or multiple flags |
| `-e, --ext <extensions>` | Filter by extensions (`.rs` and `rs` both work) |
| `-t, --trigger <path>` | Trigger file for auto-restart (see below) |
| `-- <command...>` | Command to run |

## Config file

`rewatch.toml` fields:

```toml
command = "cargo run --release"     # command to execute (shell-style quoting supported)
watch = ["src", "Cargo.toml"]       # files/directories to watch
ext = ["rs", "toml"]                # filter by extension (optional)
trigger = ".rewatch-trigger"        # trigger file (optional)

[env]                               # environment variables for the child process
RUST_LOG = "debug"
SQLX_MIGRATE_IGNORE_MISSING = "true"
```

## Trigger file

The trigger file enables **automated restarts without pressing Enter**. When rewatch detects that the trigger file was created or modified, it restarts the command immediately.

```toml
trigger = ".rewatch-trigger"
```

Use case: add an instruction to your `CLAUDE.md` so that Claude creates this file after making changes, triggering an automatic rebuild:

```
touch .rewatch-trigger
```

Add the trigger file to `.gitignore`.

## Platform support

- **Windows**: kills process tree via Win32 Job Objects
- **Linux**: kills process group via `kill(-pgid, SIGTERM)`
- **macOS**: same as Linux

Powered by [notify](https://crates.io/crates/notify) (OS-native file system events) and [process-wrap](https://crates.io/crates/process-wrap) (from the [watchexec](https://github.com/watchexec/watchexec) project).

## License

MIT
