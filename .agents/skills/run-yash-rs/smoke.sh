#!/bin/bash
# Smoke script for yash3 (yash-rs shell binary).
# Builds the binary then runs representative invocations and checks output/exit codes.
# Run from the repository root.

set -e

BINARY=./target/debug/yash3

echo "=== Building yash3 ==="
cargo build --package yash-cli

echo
echo "=== Version ==="
$BINARY --version

echo
echo "=== Basic execution (-c) ==="
out=$($BINARY -c 'echo hello from yash')
[ "$out" = "hello from yash" ] || { echo "FAIL: expected 'hello from yash', got '$out'"; exit 1; }
echo "PASS: echo works"

echo
echo "=== Variable expansion ==="
out=$($BINARY -c 'x=42; echo "x is $x"')
[ "$out" = "x is 42" ] || { echo "FAIL: $out"; exit 1; }
echo "PASS: variable expansion works"

echo
echo "=== Arithmetic expansion ==="
out=$($BINARY -c 'echo $((3 + 4 * 2))')
[ "$out" = "11" ] || { echo "FAIL: $out"; exit 1; }
echo "PASS: arithmetic expansion works"

echo
echo "=== For loop ==="
out=$($BINARY -c 'for i in a b c; do echo "$i"; done')
[ "$out" = "$(printf 'a\nb\nc')" ] || { echo "FAIL: $out"; exit 1; }
echo "PASS: for loop works"

echo
echo "=== If/then/else ==="
out=$($BINARY -c 'if true; then echo yes; else echo no; fi')
[ "$out" = "yes" ] || { echo "FAIL: $out"; exit 1; }
echo "PASS: conditionals work"

echo
echo "=== Pipe ==="
out=$($BINARY -c 'echo "hello" | cat')
[ "$out" = "hello" ] || { echo "FAIL: $out"; exit 1; }
echo "PASS: pipes work"

echo
echo "=== Stdin input ==="
out=$(echo 'echo "from stdin"' | $BINARY)
[ "$out" = "from stdin" ] || { echo "FAIL: $out"; exit 1; }
echo "PASS: stdin input works"

echo
echo "=== Script file ==="
tmp=$(mktemp /tmp/yash_smoke_XXXXXX.sh)
cat > "$tmp" << 'SCRIPT'
name="world"
echo "Hello, $name!"
SCRIPT
out=$($BINARY "$tmp")
rm -f "$tmp"
[ "$out" = "Hello, world!" ] || { echo "FAIL: $out"; exit 1; }
echo "PASS: script file works"

echo
echo "=== Exit codes ==="
$BINARY -c 'exit 0' && rc=$? || rc=$?; [ "$rc" -eq 0 ] || { echo "FAIL: exit 0 returned $rc"; exit 1; }
$BINARY -c 'exit 1' && rc=$? || rc=$?; [ "$rc" -eq 1 ] || { echo "FAIL: exit 1 returned $rc"; exit 1; }
$BINARY -c 'false' && rc=$? || rc=$?; [ "$rc" -eq 1 ] || { echo "FAIL: false returned $rc"; exit 1; }
echo "PASS: exit codes correct"

echo
echo "=== Command-not-found gives 127 ==="
$BINARY -c 'command_that_does_not_exist_xyz' 2>/dev/null && rc=$? || rc=$?
[ "$rc" -eq 127 ] || { echo "FAIL: expected 127, got $rc"; exit 1; }
echo "PASS: command-not-found gives 127"

echo
echo "=== Syntax error gives exit 2 ==="
$BINARY -c 'if then done' 2>/dev/null && rc=$? || rc=$?
[ "$rc" -eq 2 ] || { echo "FAIL: expected 2, got $rc"; exit 1; }
echo "PASS: syntax errors give exit 2"

echo
echo "=== Noexec flag (-n) parses without executing ==="
out=$($BINARY -n -c 'echo should_not_print'; echo $?)
[ "$out" = "0" ] || { echo "FAIL: noexec returned '$out'"; exit 1; }
echo "PASS: -n flag works"

echo
echo "=== All checks passed ==="
