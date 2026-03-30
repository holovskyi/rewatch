# rewatch

A smarter **cargo-watch** alternative, designed for AI coding agents.

## The problem with cargo-watch

Tools like `cargo-watch` rebuild on **every file save**. When an AI agent (Claude Code, Cursor, Copilot) edits multiple files in rapid succession, this triggers dozens of redundant builds — wasting CPU and producing noise.

**rewatch** takes a different approach:

1. Detects file changes and **kills the running process**
2. **Waits for Enter** (or a trigger file) before restarting — so the agent can finish all its edits first
3. Restarts **once**, when you're actually ready

This means one clean build instead of twenty failed ones.

## Install

```bash
cargo install rewatch
```

## Quick start

```bash
# CLI arguments
rewatch -w src,Cargo.toml -e rs,toml -- cargo run

# Or with a config file (just run `rewatch` with no arguments)
rewatch
```

### Config file

Create `rewatch.toml` in your project root:

```toml
command = "cargo run"
watch = ["src", "Cargo.toml"]
ext = ["rs", "toml"]

[env]
RUST_LOG = "debug"
```

CLI arguments override config file values.

## Using with AI coding agents

The **trigger file** feature enables a fully automated edit-build-test loop:

1. Run `rewatch` in one terminal
2. The AI agent edits your code — rewatch detects changes and kills the running process
3. When the agent is done, it touches the trigger file — rewatch restarts immediately (no Enter needed)
4. The agent sees build output and iterates

### Setup with Claude Code

`rewatch.toml`:

```toml
command = "cargo run"
watch = ["src"]
ext = ["rs", "toml"]
trigger = ".rewatch-trigger"
```

`CLAUDE.md`:

```
After making code changes that require a rebuild, run: touch .rewatch-trigger
```

`.gitignore`:

```
.rewatch-trigger
```

Run `rewatch` in one terminal and Claude Code in another — they work together automatically.

## How it works

1. Starts your command
2. Watches files for changes (event-driven, not polling)
3. On change — kills the process (entire tree) and shows diff-style indicators:
   - `+` created, `~` modified, `-` removed
4. Waits for **Enter** before restarting (so you or the agent can finish edits)
5. On process crash — shows exit code, waits for Enter
6. On trigger file — restarts immediately without Enter

## CLI options

| Option | Description |
|---|---|
| `-w, --watch <paths>` | Paths to watch, comma-separated or multiple flags |
| `-e, --ext <extensions>` | Filter by extensions (`.rs` and `rs` both work) |
| `-t, --trigger <path>` | Trigger file for auto-restart |
| `-- <command...>` | Command to run |

## Config file reference

```toml
command = "cargo run --release"     # command to execute (shell-style quoting supported)
watch = ["src", "Cargo.toml"]       # files/directories to watch
ext = ["rs", "toml"]                # filter by extension (optional)
trigger = ".rewatch-trigger"        # trigger file (optional)

[env]                               # environment variables for the child process
RUST_LOG = "debug"
SQLX_MIGRATE_IGNORE_MISSING = "true"
```

## Platform support

- **Windows**: kills process tree via Win32 Job Objects
- **Linux**: kills process group via `kill(-pgid, SIGTERM)`
- **macOS**: same as Linux

Powered by [notify](https://crates.io/crates/notify) (OS-native file system events) and [process-wrap](https://crates.io/crates/process-wrap) (from the [watchexec](https://github.com/watchexec/watchexec) project).

## rewatch vs cargo-watch

| | cargo-watch | rewatch |
|---|---|---|
| Rebuilds on every save | Yes | No — waits for Enter or trigger |
| AI agent friendly | No — floods with builds | Yes — trigger file for automation |
| Kills process tree | Partial | Full (Job Objects / process groups) |
| Config file | No | `rewatch.toml` |
| Language-agnostic | Cargo only | Any command |

## License

MIT
