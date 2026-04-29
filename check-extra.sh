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
RUSTFLAGS='-D unused_crate_dependencies' cargo check --lib --all-features
RUSTFLAGS='-D unused_crate_dependencies' cargo check --package 'yash-builtin' --no-default-features
RUSTFLAGS='-D unused_crate_dependencies' cargo check --package 'yash-builtin' --no-default-features --features yash-semantics

# Make sure the crates can be built with all combinations of features.
cargo build --release --package 'yash-arith' --all-targets
cargo build --release --package 'yash-builtin' --all-targets
cargo build --release --package 'yash-builtin' --all-targets --no-default-features
cargo build --release --package 'yash-builtin' --all-targets --no-default-features --features yash-semantics
cargo build --release --package 'yash-cli' --all-targets
cargo build --release --package 'yash-env' --all-targets
cargo build --release --package 'yash-env' --all-targets --no-default-features
cargo build --release --package 'yash-env' --all-targets --no-default-features --features test-helper
cargo build --release --package 'yash-env-test-helper' --all-targets
cargo build --release --package 'yash-executor' --all-targets
cargo build --release --package 'yash-fnmatch' --all-targets
cargo build --release --package 'yash-prompt' --all-targets
cargo build --release --package 'yash-quote' --all-targets
cargo build --release --package 'yash-semantics' --all-targets
cargo build --release --package 'yash-syntax' --all-targets

# Test with non-default feature configurations.
#cargo test --package 'yash-arith' -- $quiet
#cargo test --package 'yash-builtin' -- $quiet
cargo test --package 'yash-builtin' --features yash-semantics -- $quiet
#cargo test --package 'yash-cli' -- $quiet
#cargo test --package 'yash-env' -- $quiet
cargo test --package 'yash-env' --features test-helper -- $quiet
#cargo test --package 'yash-fnmatch' -- $quiet
#cargo test --package 'yash-prompt' -- $quiet
#cargo test --package 'yash-quote' -- $quiet
#cargo test --package 'yash-semantics' -- $quiet
#cargo test --package 'yash-syntax' -- $quiet

# Make sure next releases have correct semantic versions.
cargo semver-checks --package 'yash-arith'
cargo semver-checks --package 'yash-builtin'
cargo semver-checks --package 'yash-env'
cargo semver-checks --package 'yash-env-test-helper'
cargo semver-checks --package 'yash-executor'
cargo semver-checks --package 'yash-fnmatch'
cargo semver-checks --package 'yash-prompt'
cargo semver-checks --package 'yash-quote'
cargo semver-checks --package 'yash-semantics'
cargo semver-checks --package 'yash-syntax'
