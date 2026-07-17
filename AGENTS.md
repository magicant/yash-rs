# AGENTS.md

This repository contains the yash-rs project, a POSIX-compliant shell written in Rust.

## Scope and priorities

- Preserve POSIX shell behavior unless the task explicitly changes it.
- Keep changes minimal and crate-local when possible.
- Prefer links to canonical docs over duplicating policy text.

## Quick start commands

Run from repository root.

- Full routine checks: `./check.sh`
- Extra checks (TOML/lints/features): `./check-extra.sh`
- SemVer checks for workspace crates: `./check-semver.sh`
- Release feature-matrix checks: `./check-release-build.sh`
- Per-crate MSRV checks: `./check-msrv.sh <crate>`
- Build and check the user manual: `./check-docs.sh`

## Workspace map

This workspace contains `yash-*` crates.

- [yash-cli](yash-cli/README.md): shell binary (`yash3`) and CLI integration.
- [yash-syntax](yash-syntax/README.md): shell language syntax and parser.
- [yash-semantics](yash-semantics/README.md): shell language semantics.
- [yash-builtin](yash-builtin/README.md): built-in utilities.
- [yash-env](yash-env/README.md): shell execution environment/state.
- [yash-arith](yash-arith/README.md): arithmetic expansion.
- [yash-prompt](yash-prompt/README.md): command-line prompt rendering.
- [yash-executor](yash-executor/README.md): single-threaded futures executor.
- [yash-fnmatch](yash-fnmatch/README.md): POSIX-style glob matching.
- [yash-quote](yash-quote/README.md): shell quoting utility.

## Testing expectations

- Follow Kent Beck's Canon TDD when implementing a feature or fixing a bug.
- Put unit tests in the same file as the code they test, using `#[cfg(test)] mod tests { ... }`.
- If shell-observable behavior changes, add/update scripted tests under [yash-cli/tests/scripted_test](yash-cli/tests/scripted_test/).
- The scripted test harness entry point is [yash-cli/tests/scripted_test.rs](yash-cli/tests/scripted_test.rs).

## Versioning and changelog rules

For behavior/API changes, use the [`bump-versions` skill](.agents/skills/bump-versions/SKILL.md), which is the canonical procedure for the workspace's versioning rules. Also use the [`update-changelog` skill](.agents/skills/update-changelog/SKILL.md) to update the changelog for the affected crate(s).

If behavior changes are user-visible, update docs in [docs/src](docs/src/) (handled by the [`update-docs` skill](.agents/skills/update-docs/SKILL.md)).

## Commit history arrangement

Every commit should be clean and self-contained. Run `cargo fmt`, `./check.sh`, and optionally other relevant checks before committing. Tests, documentation, changelog, and version bumps should be included in the **same commit** as the code change.

Commits should be arranged so that each commit focuses on a single logical concern.

Desired commit history example:

- Commit 1: Introduce a new API in crate A, add tests, and update docs/changelog/version for crate A.
- Commit 2: Modify crate B to call the new API in crate A, add tests, and update docs/changelog/version for crate B.
- Commit 3: Refactor crate B implementation without modifying existing tests to ensure the refactor is behavior-preserving.

Unwanted commit history example:

- Commit 1: Introduce a new API in crate A and modify crate B to call it, but do not add tests or update docs/changelog/version for either crate.
- Commit 2: Add tests for crates A and B, but do not update docs/changelog/version for either crate.
- Commit 3: Update docs/changelog for crates A and B, but do not update versions for either crate.

## Review focus for code changes

Use repository-specific review criteria in [.github/instructions/code-review.instructions.md](.github/instructions/code-review.instructions.md).

## Useful references

- Project overview: [README.md](README.md)
- User/developer docs index: [docs/src/README.md](docs/src/README.md)
- Versioning policy docs: [docs/src/versioning.md](docs/src/versioning.md)
- Release automation script: [do-release.sh](do-release.sh)
