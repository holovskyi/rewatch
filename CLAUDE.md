# Rewatch

Cross-platform file watcher with process restart for AI coding agents.

## Release process

When publishing a new version:

1. Review and update `README.md` to reflect the current state
2. Update version in `Cargo.toml`
3. Review commits since last release: `git log v<last-version>..HEAD --oneline`
4. Add a new section to `CHANGELOG.md` based on the actual commits
5. Commit, push
6. `cargo publish`
7. Tag the release commit: `git tag v<version>` and `git push --tags`
