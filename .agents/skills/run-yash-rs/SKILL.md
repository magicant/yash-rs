---
name: run-yash-rs
description: Build, run, and smoke-test yash3 (yash-rs POSIX shell). Use when asked to run yash3, build the shell binary, test it, verify a shell behavior change, or run the smoke script.
---

`yash-rs` is a POSIX shell implementation in Rust. The primary artifact is the `yash3` binary produced by the `yash-cli` crate. Drive it via `.claude/skills/run-yash-rs/smoke.sh` (a smoke script that builds and runs representative invocations) or invoke the binary directly for one-off checks.

All paths below are relative to the repository root.

## Prerequisites

Rust toolchain (already present if `cargo` is on PATH — no extra packages needed).

## Build

```bash
cargo build --package yash-cli
# binary lands at: ./target/debug/yash3
```

For a release build:

```bash
cargo build --release --package yash-cli
# binary lands at: ./target/release/yash3
```

## Run (agent path)

Run the smoke script to build and verify the binary end-to-end:

```bash
bash .claude/skills/run-yash-rs/smoke.sh
# → prints PASS for each check, exits 0 on success
```

For one-off invocations:

```bash
./target/debug/yash3 --version
# → yash 3.1.0

# Run a shell snippet:
./target/debug/yash3 -c 'echo hello; x=42; echo "x is $x"'

# Run a script file:
./target/debug/yash3 /path/to/script.sh

# Read script from stdin:
echo 'echo "from stdin"' | ./target/debug/yash3

# Check syntax without executing (-n = noexec):
./target/debug/yash3 -n -c 'for i in a b c; do echo "$i"; done'
```

Exit codes: `0` success, `1` general failure / `false`, `2` syntax error, `127` command not found.

## Run (human path)

```bash
./target/debug/yash3   # → drops into an interactive POSIX shell session. Ctrl-D to exit.
```

## Test

```bash
# Unit + integration tests for yash-cli only (fast, ~3 s):
cargo test --package yash-cli

# Full workspace tests:
cargo test

# Scripted POSIX-conformance tests live under yash-cli/tests/scripted_test/
# and are run as part of cargo test --package yash-cli.
```

## Gotchas

- **`--help` panics** — `yash3 --help` is not implemented and panics with "not yet implemented: print help". Use `--version` to confirm the binary works.
- **`set -e` in wrapper scripts swallows yash3 exit codes** — if your wrapper uses `set -e`, use the `cmd && rc=$? || rc=$?` pattern to capture non-zero exits without triggering early termination (see `smoke.sh`).
- **Interactive mode needs a real TTY** — piping to `yash3` runs non-interactively (no prompt, no job control). To test interactive features, use `script` or `expect`; the scripted test suite (`yash-cli/tests/scripted_test/`) covers this automatically via a PTY harness.

## Troubleshooting

- **`cargo build` fails with missing dependencies**: run `rustup update` — the workspace specifies `rust-version = "1.96.0"`.
- **`command_that_does_not_exist` exit code in a `set -e` script**: the shell returns 127 but `set -e` in the *calling* bash script may abort before you can check `$?`. Use `|| rc=$?` to capture it (see Gotchas above).
