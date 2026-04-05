# Changelog

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
