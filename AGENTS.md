# AGENTS.md

Guidance for AI coding agents working in this repository.

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
- Per-crate release readiness: `./check-release-readiness.sh <crate>`
- Per-crate MSRV checks: `./check-msrv.sh <crate>`

Common direct commands:

- `cargo fmt -- --check`
- `cargo test`
- `cargo clippy --all-targets`
- `cargo doc --no-deps`

## Workspace map

This workspace contains `yash-*` crates. See root [Cargo.toml](Cargo.toml) for current members/versions.

- [yash-cli](yash-cli/README.md): shell binary (`yash3`) and CLI integration.
- [yash-syntax](yash-syntax/README.md): parser and alias substitution.
- [yash-semantics](yash-semantics/README.md): shell language semantics.
- [yash-builtin](yash-builtin/README.md): built-in utilities.
- [yash-env](yash-env/README.md): shell execution environment/state.
- [yash-arith](yash-arith/README.md): POSIX arithmetic expansion.
- [yash-prompt](yash-prompt/README.md): prompt rendering.
- [yash-executor](yash-executor/README.md): single-threaded futures executor (dual licensed).
- [yash-fnmatch](yash-fnmatch/README.md): POSIX-style glob matching (dual licensed).
- [yash-quote](yash-quote/README.md): shell quoting utility (dual licensed).

## Testing expectations

- Put unit tests near changed code when practical.
- If shell-observable behavior changes, add/update scripted tests under [yash-cli/tests/scripted_test](yash-cli/tests/scripted_test/).
- The scripted test harness entry point is [yash-cli/tests/scripted_test.rs](yash-cli/tests/scripted_test.rs).

## Versioning and changelog rules

For behavior/API changes, follow the PR checklist in
[.github/pull_request_template.md](.github/pull_request_template.md):

- Bump affected crate versions in each crate `Cargo.toml`.
- Sync workspace dependency versions in root [Cargo.toml](Cargo.toml).
- Update affected crate changelogs (`[x.y.z] - Unreleased`, categorized entries).
- If behavior changes are user-visible, update docs in [docs/src](docs/src/).

## Review focus for code changes

Use repository-specific review criteria in
[.github/instructions/code-review.instructions.md](.github/instructions/code-review.instructions.md), especially:

- POSIX compliance and shell semantics correctness.
- Edge cases and error handling quality.
- Signal/job-control correctness.
- Trait-bound minimization in `yash-builtin` and `yash-semantics`.
- License compatibility (GPL crates vs dual-licensed utility crates).

## Useful references

- Project overview: [README.md](README.md)
- User/developer docs index: [docs/src/README.md](docs/src/README.md)
- Versioning policy docs: [docs/src/versioning.md](docs/src/versioning.md)
- Release automation script: [do-release.sh](do-release.sh)

## Safe change workflow (recommended)

Before writing any code for a task that will produce git commits, invoke the
`commit` skill via the `Skill` tool. The skill drives the work as an
incremental loop — one logical step at a time, each passing `./check.sh`
before being committed. Do not write all the code first and commit at the end.

Per-step loop:

1. Read relevant crate README and docs section.
2. Implement minimal changes for this step.
3. Run `cargo fmt`, then `./check.sh` (must exit 0 with no warnings).
4. Stage and commit the step.
5. If versions or public behavior changed, update changelog/docs and run semver/release checks.
