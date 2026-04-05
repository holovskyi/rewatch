# Rewatch

Cross-platform file watcher with process restart for AI coding agents.

## Release process

When publishing a new version:

1. Review and update `README.md` to reflect the current state
2. Update version in `Cargo.toml`
3. Add a new section to `CHANGELOG.md` with the changes
4. Commit, push
5. `cargo publish`
6. Tag the release commit: `git tag v<version>` and `git push --tags`
