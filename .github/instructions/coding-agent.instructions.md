---
applyTo: "**"
excludeAgent: "code-review"
---

# Yash-rs Copilot Instructions

## Project Overview

**Yash-rs** is a reimplementation of [Yet Another Shell (yash)](https://magicant.github.io/yash/) in Rust. It's a POSIX-compatible shell with ~104,000 lines of Rust code across 272 source files, organized as a Cargo workspace with 11 crates.

**Key Facts:**
- Main binary: `yash3` (from `yash-cli` crate)
- Target platforms: Unix-like systems (Linux, macOS, WSL on Windows)
- Rust version: 1.90.0 (stable), MSRV: 1.87.0
- License: GPLv3 for most crates; MIT/Apache-2.0 for `yash-executor`, `yash-fnmatch`, `yash-quote`

## Workspace Structure

This is a Cargo workspace with the following crates (all in the workspace root (`.`)):

- **yash-cli**: Main executable binary, produces `yash3` command
- **yash-syntax**: POSIX shell script parser
- **yash-semantics**: Shell semantics implementation
- **yash-env**: Shell environment and state management
- **yash-builtin**: Built-in utilities (cd, echo, etc.)
- **yash-executor**: Command execution (MIT/Apache-2.0)
- **yash-fnmatch**: Pattern matching (MIT/Apache-2.0)
- **yash-quote**: String quoting utilities (MIT/Apache-2.0)
- **yash-arith**: Arithmetic expression evaluation
- **yash-prompt**: Prompt rendering
- **yash-env-test-helper**: Test utilities for environment

Each crate has:
- `Cargo.toml` - Package configuration
- `CHANGELOG.md` - Version history
- `README.md` - Crate description
- `LICENSE-*` - License files
- `src/` - Source code

## Build & Test Commands

### Essential Commands (run these first)

**Format, test, doc, and lint (basic validation):**
```sh
./check.sh
# OR for verbose output:
./check.sh -v
```
Time: ~1.5-2 minutes
This runs:
1. `cargo fmt -- --check` - Verify formatting
2. `cargo test -- --quiet` - Run all tests (quiet mode)
3. `cargo doc` - Build documentation
4. `cargo clippy --all-targets` - Lint with Clippy

### Individual Build Commands

**Build all crates (dev):**
```sh
cargo build
```
Time: ~7-8 seconds (after initial build)

**Build release version:**
```sh
cargo build --release --package yash-cli
```
Time: ~40 seconds (produces optimized binary at `target/release/yash3`)

**Run tests:**
```sh
cargo test
# OR for specific package:
cargo test --package yash-cli
# OR for scripted integration tests:
cargo test --package yash-cli --test scripted_test
```
Time: ~45 seconds for full test suite, ~10 seconds for scripted tests

**Format code:**
```sh
cargo fmt
# Check formatting without modifying:
cargo fmt -- --check
```

**Lint with Clippy:**
```sh
cargo clippy --all-targets
```
Time: ~12 seconds

**Build documentation:**
```sh
cargo doc
```
Time: ~17 seconds (output in `target/doc/`)

### Extended Validation (CI checks)

**Extra checks (requires additional tools):**
```sh
./check-extra.sh -v
```
**Prerequisites:** Install first with:
```sh
cargo install taplo-cli cargo-semver-checks
```
This script verifies:
- TOML formatting (all Cargo.toml files)
- Unused dependencies check
- Feature combination builds for all crates
- Semantic versioning compliance (semver-checks)

**MSRV checks (Minimum Supported Rust Version):**
```sh
./check-msrv.sh -v
```
**Prerequisites:** Requires nightly and 1.87.0 toolchains
Tests each crate with minimal dependency versions at their MSRV.

**Documentation checks:**
```sh
./check-docs.sh
```
**Prerequisites:** Install with:
```sh
cargo install mdbook --no-default-features --features search --version "^0.4" --locked
```
This builds the mdBook documentation in `docs/` and runs doctests.

## Running the Shell

Test the built shell:
```sh
./target/release/yash3 --version
./target/release/yash3 -c 'echo "Hello from yash3"'
```

## CI/CD Workflows

The CI pipeline (`.github/workflows/ci.yml`) runs on push/PR to master with 6 jobs:

1. **check**: Runs `./check.sh -v` with `-D warnings` (fail on warnings)
2. **clippy**: Lints code with GitHub PR review integration
3. **extra**: Runs `./check-extra.sh -v` (requires taplo-cli, cargo-semver-checks)
4. **msrv**: Tests MSRV on Ubuntu + macOS with `./check-msrv.sh -v`
5. **windows**: Tests portable crates on Windows (yash-arith, yash-executor, yash-fnmatch, yash-quote, yash-syntax, yash-env, etc.)
6. **docs**: Builds and tests documentation with `./check-docs.sh`

**All jobs must pass for CI to succeed.**

## Making Code Changes

### Version Bumping Rules (CRITICAL)

When making changes, **update version numbers** in affected crates' `Cargo.toml` according to the type of change:
- **Patch**: Bug fixes, internal changes
- **Minor**: New features, non-breaking API additions
- **Major**: Breaking changes

For library crates: API changes drive version bumps.
For yash-cli: Observable behavior changes drive version bumps.

**Important**: `Cargo.toml` should always forecast the next release version. **Avoid double version bumps** - if a version was already bumped in a previous merged PR that hasn't been released yet, do not bump it again.

**Two files to update per affected crate:**
1. `<crate>/Cargo.toml` - Update `version = "x.y.z"`
2. Root `Cargo.toml` - Update workspace dependency version

### Changelog Requirements

Add `[x.y.z] - Unreleased` section to `<crate>/CHANGELOG.md` if not present, with changes grouped by:
- Added
- Changed
- Deprecated
- Removed
- Fixed
- Security

For yash-cli CHANGELOG: Include changes in observable behavior even if yash-cli code didn't change directly.

### Test Requirements

**Unit tests**: Add in same file as code being tested
**Integration tests**: Update the Rust test harness `yash-cli/tests/scripted_test.rs` for observable shell behavior changes. Add or modify shell script test cases in the `yash-cli/tests/scripted_test/` directory (contains `.sh` files executed by the harness).

### Style Guidelines

- Code style: Use existing Rust conventions, verified by `cargo fmt`
- Clippy must pass without warnings
- Match existing patterns in the codebase
- All public APIs must have documentation comments, except for trivial trait implementations
    - If the documentation comment for an item starts with a paragraph composed only of a noun phrase, do not end that paragraph with a period. Other paragraphs should end with periods.

## Key Configuration Files

- **Cargo.toml** (root): Workspace configuration, shared dependencies
- **.gitignore**: Excludes `./docs/book` and `./target`
- **.markdownlint.json**: Markdown linting rules
- **docs/book.toml**: mdBook configuration for documentation
- **.github/workflows/ci.yml**: Main CI pipeline
- **workspace.code-workspace**: VS Code workspace with custom spell check dictionary

## File Structure at Root

```
.
├── .github/              # GitHub workflows and templates
├── .vscode/              # VS Code configurations
├── docs/                 # mdBook documentation source
├── yash-*/               # 11 crate directories
├── Cargo.toml            # Workspace configuration
├── Cargo.lock            # Dependency lock file
├── README.md             # Project readme
├── check*.sh             # Validation scripts
├── do-release.sh         # Release automation
└── workspace.code-workspace
```

## Important Notes

1. **Always run `./check.sh` before committing** to catch formatting, tests, docs, and clippy issues.

2. **Tests run quietly by default** (`--quiet` flag). Use `-v` flag with check scripts for verbose output.

3. **Windows support is limited**: Only specific crates (executor, fnmatch, quote, syntax, env, arith) are tested on Windows. Most shell functionality requires Unix-like systems.

4. **Feature flags matter**: Some crates have feature combinations that must all build correctly. `yash-builtin` has `default = ["yash-prompt", "yash-semantics"]` with conditional features. `yash-syntax` has optional `annotate-snippets` feature.

5. **Don't add dependencies lightly**: The `check-extra.sh` script enforces no unused dependencies with `RUSTFLAGS='-D unused_crate_dependencies'`.

6. **Formatting is strict**: All Cargo.toml files must be formatted and linted with `taplo`.

7. **Scripted tests are integration tests**: Located in `yash-cli/tests/scripted_test/`, these test actual shell behavior with `.sh` files.

8. **Build artifacts**: Git ignores `./target` and `./docs/book`. Don't commit build artifacts.

9. **Documentation is versioned**: When adding features, mention the yash-cli version number in docs (unless it's a bug fix).

10. **Semantic versioning is enforced**: `cargo semver-checks` runs in CI to prevent accidental breaking changes.

## Trust These Instructions

These instructions are comprehensive and tested. Only search the codebase if:
- Information here is incomplete or unclear
- You need specifics about a particular module's implementation
- Instructions appear outdated or incorrect

For standard build/test/validation workflows, follow these instructions directly without additional exploration.
