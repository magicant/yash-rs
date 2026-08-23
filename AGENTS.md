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

## Testing expectations

- Follow Kent Beck's Canon TDD when implementing a feature or fixing a bug. Canon TDD ends with a refactoring phase; do not stop at a green test. This will guide you to the right amount of tests (neither too many nor too few). Use the [`unit-tests` skill](.agents/skills/unit-tests/SKILL.md) when writing the tests; it is the canonical guide to how unit tests are organized, named, and implemented in this workspace.
- Put unit tests in the same file as the code they test, using `#[cfg(test)] mod tests { ... }`.
- If shell-observable behavior changes, add/update scripted tests under [yash-cli/tests/scripted_test](yash-cli/tests/scripted_test/).
- The scripted test harness entry point is [yash-cli/tests/scripted_test.rs](yash-cli/tests/scripted_test.rs), which invokes [yash-cli/tests/scripted_test/run-test.sh](yash-cli/tests/scripted_test/run-test.sh) as the test driver.
- In a scripted test, prefer letting the harness compare the result: print what the test produces and declare the expectation in a `__OUT__`/`__ERR__` section (`test_o`, `test_oE`, `test_oe`, …) rather than asserting inside the test case with `[ ... ]` and `test_x`. A harness comparison shows the expected and actual output when it fails, whereas an in-case assertion only reports a non-zero exit status. Assert inside the case only where the harness cannot express the check.

## Code style

- Do not write a comment that only restates what the code plainly does. Reserve comments for what the code cannot say: a non-obvious reason, a specification reference, or a caveat.
- Where a function runs through many consecutive blocks, a short comment naming what a block does (for example, `// Check portability of - and --`) helps the reader scan. Keep it a label, not an explanation.

## Refactoring

Every change ends with a refactoring pass, not with the first passing test.

- Look beyond the lines you added or edited. The surrounding untouched code in the
  same function or module is in scope: a new branch often makes an existing one
  redundant, or reveals a simpler shape for the whole block.
- When goals conflict, apply this order of priority:
  1. Remove avoidable work — clones, allocations, redundant traversals, and other
     computation the code does not need to do.
  2. Keep the code readable.
  3. Keep the code short.
- Prefer the idiom the standard library already offers over an open-coded
  equivalent; it is usually shorter and cheaper at once. For example, consume a
  `Peekable` with `next_if` or `next_if_map` rather than `peek()` followed by
  `next().unwrap()`.
- A refactor must never change behavior. Where it lands depends on its reach, not on
  the red-green-refactor cycle — do not commit once per cycle.
    - Tidying that stays within the code this change introduces belongs in the same
      commit as the change. The commit should present the feature already in its
      final shape.
    - A refactor that reaches into pre-existing code the change did not require, or
      that grows large enough that a reviewer would struggle to tell the new behavior
      from the rearrangement, becomes its own commit — with the existing tests
      unmodified, so the commit itself demonstrates that behavior is preserved.

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
