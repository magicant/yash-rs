set -Ceu

if [ "$*" = "" ]; then quiet='--quiet'; else quiet=''; fi

set -x

# Make sure Cargo.lock is clean before running cargo update
git diff --exit-code -- Cargo.lock
trap 'git checkout -- Cargo.lock' EXIT

cargo +nightly update -Z direct-minimal-versions

cargo +1.60.0 test --package 'yash-arith' -- $quiet
cargo +1.67.0 test --package 'yash-builtin' -- $quiet
cargo +1.67.0 test --package 'yash-env' -- $quiet
cargo +1.60.0 test --package 'yash-fnmatch' -- $quiet
cargo +1.60.0 test --package 'yash-quote' -- $quiet
cargo +1.67.0 test --package 'yash-semantics' -- $quiet
cargo +1.67.0 test --package 'yash-syntax' -- $quiet
cargo +1.67.0 test --package 'yash-syntax' --features annotate-snippets -- $quiet
cargo +1.67.0 test --package 'yash' -- $quiet
