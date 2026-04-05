# Rewatch

Cross-platform file watcher with process restart for AI coding agents.

## Release process

When publishing a new version:

1. Update version in `Cargo.toml`
2. Add a new section to `CHANGELOG.md` with the changes
3. Commit, push
4. `cargo publish`
5. Tag the release commit: `git tag v<version>` and `git push --tags`
