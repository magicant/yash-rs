## Description

<!-- Please write a brief description of the issue you are addressing and the solution you are proposing -->

## Checklist

<!-- If you are unsure about any of these items, please ask for clarification -->

- Implementation
    - [ ] Code should follow the existing style and conventions
- Tests
    - [ ] Unit tests should be added in the same file as the code being tested
    - [ ] If the change affects observable behavior of the shell executable, scripted tests should be added or updated (`yash-cli/tests/scripted_test.rs`)
- Versioning
    - [ ] The version number in `Cargo.toml` for the affected crates should be updated according to the type of change (patch, minor, major) so that `Cargo.toml` forecasts the next release version
        - For library crates other than `yash-cli`, changes in public API affect the version number
            - If a crate re-exports items from a dependency, bumping the dependency's major/minor version should also bump the crate's major/minor version
        - For the `yash-cli` binary crate, changes in observable behavior affect the version number
        - Avoid double version bumps if already done in a previous PR
        - If a PR affects multiple crates, all affected crates should have their version numbers updated accordingly
    - [ ] The root `Cargo.toml` should be updated to reflect the new version numbers of the affected crates
- Changelog
    - [ ] The `[x.y.z] - Unreleased` heading should be added to `CHANGELOG.md` of affected crates if it does not already exist, where `x.y.z` is the next version to be released
        - If the changes in the PR affect observable behavior of the `yash-cli` binary, the `[x.y.z] - Unreleased` heading should also be added to `CHANGELOG.md` of `yash-cli` regardless of whether `yash-cli` itself is being updated
    - [ ] The Unreleased section should contain the changes made in this PR, grouped by type (Added, Changed, Deprecated, Removed, Fixed, Security)
        - For library crates other than `yash-cli`, `CHANGELOG.md` should contain changes in public API
        - For the `yash-cli` binary crate, `CHANGELOG.md` should contain changes in observable behavior
    - [ ] If a dependency has been added, removed, or updated in `Cargo.toml`, it should be mentioned in the changelog
        - Private and public dependencies should be mentioned separately. Private dependencies are crates whose items are not re-exported by the dependent crate. Bumping a private dependency version does not require a version bump of the dependent crate.
        - For example, if you add a public function in `yash-syntax`, bumping its minor version, then `CHANGELOG.md` of `yash-syntax` should mention the new function, and `CHANGELOG.md` of all crates depending on `yash-syntax` should mention that `yash-syntax` has been updated to the new version.
- Documentation
    - [ ] The documentation (`docs/src`) should be updated to reflect the new behavior
    - [ ] The documentation should mention the version number of `yash-cli` that introduces the new behavior (unless it is a bug fix)
