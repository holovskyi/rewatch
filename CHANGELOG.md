# Changelog

## [0.4.2] - 2026-05-12

### Added
- `-c`/`--config <PATH>` CLI flag to point at a custom config file. When given explicitly, the file is required; without it, `rewatch.toml` from cwd is used if present (CLI-only mode preserved when absent).
- Hot-reload: rewatch watches its own config file. Editing `rewatch.toml` (or the file passed via `--config`) live-reloads the config and respawns the child with the new command/env/watch paths/trigger.
- Content-hash dedup on config reload: no-op saves (`touch rewatch.toml`, `:w` without edits, duplicate notify events on Windows ReadDirectoryChangesW) are silently ignored — the running child is not killed.

### Changed
- TOML parse errors and transient file absence during runtime reload no longer exit rewatch. A warning is logged and the previously-loaded config keeps running. Startup parse errors still exit (unchanged behavior).
- Notify events containing multiple paths (e.g. config + source file in a single rename pair) now correctly emit `FileChanged` for non-config paths instead of being dropped after the config match.

### Internal
- Generalized `is_trigger` into a shared `is_path_match` helper (used for both trigger and config detection).
- Replaced `drain_pending`'s `(Vec, bool)` return with a named `DrainResult` struct so adding the `config_changed` flag is type-safe at every callsite.
- `prompt_and_wait` now returns a `WaitOutcome { Restart, Reload(Config), Exit }` enum so the compiler enforces handling at every callsite.
- Extracted `handle_wait` helper to deduplicate the five identical `match prompt_and_wait` blocks in the main loop.
- Trigger==config startup warning prevents a silently confusing config in which trigger events would never fire.

## [0.4.1] - 2026-04-09

### Fixed
- Files explicitly listed in `watch` (e.g. `.env`) now trigger restarts regardless of the `ext` filter. Previously, files without a matching extension were silently ignored.
- `Remove` events for explicit files no longer get filtered out by the ext check.

### Changed
- Cheaper hot path: `canonicalize()` is now called only when the ext filter would otherwise reject an event, not on every event.
- Internal refactor: extracted `resolve_abs` and `collect_explicit_files` helpers with unit tests.

## [0.4.0] - 2026-04-06

### Added
- `-T`/`--trigger-always` CLI flag and `trigger_always` TOML option
- First Ctrl+C kills child process and waits, second Ctrl+C exits rewatch

### Changed
- Trigger file is now ignored while the process is running (default). It only fires when rewatch is waiting for Enter. Use `trigger_always = true` or `-T` to restore old behavior.

## [0.3.0] - 2026-04-05

### Added
- `-E`/`--env` CLI flag to pass environment variables (overrides TOML `[env]`)
- Unit tests for trigger file path comparison

### Fixed
- Deduplicate file names in change output to reduce noise from AI agents
- Clean up stale trigger file on startup to prevent unexpected restarts
- Fix trigger path comparison fallback (relative vs absolute paths)

### Changed
- Extract `print_changes_deduped` helper to reduce duplication
- Pass cached CWD into FileWatcher instead of duplicate `current_dir()` call
- Clean up startup output format

## [0.2.0] - 2026-04-04

### Added
- README and `--help` with config file examples
- LICENSE and crates.io metadata
- AI agent workflow documentation

### Changed
- Diff-style change indicators (`+`/`~`/`-`) with relative paths
- Improved description — position as cargo-watch alternative for AI agents

## [0.1.0] - 2026-04-04

### Added
- Initial implementation
- Cross-platform file watcher with process restart
- TOML config file support (`rewatch.toml`)
- Trigger file for auto-restart without Enter
- Environment variables via `[env]` in config
- Extension filtering
- Debounce for rapid file changes
