set -Ceu

if [ "$*" = "" ]; then quiet='--quiet'; else quiet=''; fi

set -x

if { ! taplo help && command -v npx; } >/dev/null 2>&1; then
    taplo() { npx @taplo/cli "$@"; }
fi

# Check that all TOML files are properly formatted and linted.
taplo format --check $(git ls-files | grep '\.toml$')
taplo lint $(git ls-files | grep '\.toml$')

# Make sure we don't have any unnecessary dependencies in Cargo.toml.
CARGO_BUILD_WARNINGS=deny cargo check --all-features
CARGO_BUILD_WARNINGS=deny cargo check --package 'yash-env' --no-default-features
CARGO_BUILD_WARNINGS=deny cargo check --package 'yash-env' --no-default-features --features test-helper
CARGO_BUILD_WARNINGS=deny cargo check --package 'yash-env' --no-default-features --features yash-executor
CARGO_BUILD_WARNINGS=deny cargo check --package 'yash-builtin' --no-default-features
CARGO_BUILD_WARNINGS=deny cargo check --package 'yash-builtin' --no-default-features --features yash-semantics

# Test with non-default feature configurations.
#cargo test --package 'yash-arith' -- $quiet
#cargo test --package 'yash-builtin' -- $quiet
cargo test --package 'yash-builtin' --features yash-semantics -- $quiet
#cargo test --package 'yash-cli' -- $quiet
#cargo test --package 'yash-env' -- $quiet
cargo test --package 'yash-env' --features test-helper -- $quiet
cargo test --package 'yash-env' --features yash-executor -- $quiet
#cargo test --package 'yash-fnmatch' -- $quiet
#cargo test --package 'yash-prompt' -- $quiet
#cargo test --package 'yash-quote' -- $quiet
#cargo test --package 'yash-semantics' -- $quiet
#cargo test --package 'yash-syntax' -- $quiet
