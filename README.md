# rewatch

Cross-platform file watcher that restarts commands on changes. Designed for development workflows with AI coding agents (Claude Code, Cursor, Copilot, etc.) and manual editing alike.

Event-driven (no polling), kills entire process tree, waits for your confirmation before restarting. Supports a trigger file for fully automated restart loops with AI agents.

## Install

```bash
cargo install rewatch
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
3. On change — kills the process (entire tree) and shows what changed with diff-style indicators (`+` created, `~` modified, `-` removed)
4. Waits for **Enter** before restarting (so you can finish your edits)
5. On process crash — shows exit code, waits for Enter

## Using with AI coding agents

rewatch is designed to work seamlessly with AI coding agents like **Claude Code**, **Cursor**, **GitHub Copilot**, and others. The **trigger file** feature enables a fully automated edit-build-test loop:

1. Run `rewatch` with a trigger file configured
2. The AI agent edits your code — rewatch detects changes and kills the running process
3. When the agent is done, it creates/touches the trigger file — rewatch restarts immediately without waiting for Enter
4. The agent sees build output (errors or success) and iterates

### Setup with Claude Code

Add to your `rewatch.toml`:

```toml
command = "cargo run"
watch = ["src"]
ext = ["rs", "toml"]
trigger = ".rewatch-trigger"
```

Add to your `CLAUDE.md`:

```
After making code changes that require a rebuild, run: touch .rewatch-trigger
```

Add to `.gitignore`:

```
.rewatch-trigger
```

Now run `rewatch` in one terminal and Claude Code in another — they work together automatically.

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

This is the key feature for AI agent workflows — the agent edits code (rewatch kills the process), then touches the trigger file when ready (rewatch restarts without human intervention).

```toml
trigger = ".rewatch-trigger"
```

## Platform support

- **Windows**: kills process tree via Win32 Job Objects
- **Linux**: kills process group via `kill(-pgid, SIGTERM)`
- **macOS**: same as Linux

Powered by [notify](https://crates.io/crates/notify) (OS-native file system events) and [process-wrap](https://crates.io/crates/process-wrap) (from the [watchexec](https://github.com/watchexec/watchexec) project).

## License

MIT
