set -Ceu

if [ "$*" = "" ]; then quiet='--quiet'; else quiet=''; fi

set -x

cargo tomlfmt --dryrun --path Cargo.toml
cargo tomlfmt --dryrun --path yash-arith/Cargo.toml
cargo tomlfmt --dryrun --path yash-builtin/Cargo.toml
cargo tomlfmt --dryrun --path yash-cli/Cargo.toml
cargo tomlfmt --dryrun --path yash-env/Cargo.toml
cargo tomlfmt --dryrun --path yash-env-test-helper/Cargo.toml
cargo tomlfmt --dryrun --path yash-executor/Cargo.toml
cargo tomlfmt --dryrun --path yash-fnmatch/Cargo.toml
cargo tomlfmt --dryrun --path yash-prompt/Cargo.toml
cargo tomlfmt --dryrun --path yash-quote/Cargo.toml
cargo tomlfmt --dryrun --path yash-semantics/Cargo.toml
cargo tomlfmt --dryrun --path yash-syntax/Cargo.toml

# Make sure we don't have any unnecessary dependencies in Cargo.toml.
RUSTFLAGS='-D unused_crate_dependencies' cargo check --lib --all-features
RUSTFLAGS='-D unused_crate_dependencies' cargo check --package 'yash-builtin' --no-default-features
RUSTFLAGS='-D unused_crate_dependencies' cargo check --package 'yash-builtin' --no-default-features --features yash-semantics
RUSTFLAGS='-D unused_crate_dependencies' cargo check --package 'yash-syntax' --no-default-features

# Make sure the crates can be built with all combinations of features.
cargo build --package 'yash-arith' --all-targets
cargo build --package 'yash-builtin' --all-targets
cargo build --package 'yash-builtin' --all-targets --no-default-features
cargo build --package 'yash-builtin' --all-targets --no-default-features --features yash-semantics
cargo build --package 'yash-cli' --all-targets
cargo build --package 'yash-env' --all-targets
cargo build --package 'yash-env-test-helper' --all-targets
cargo build --package 'yash-executor' --all-targets
cargo build --package 'yash-fnmatch' --all-targets
cargo build --package 'yash-prompt' --all-targets
cargo build --package 'yash-quote' --all-targets
cargo build --package 'yash-semantics' --all-targets
cargo build --package 'yash-syntax' --all-targets
cargo build --package 'yash-syntax' --all-targets --features annotate-snippets

# Test with non-default feature configurations.
#cargo test --package 'yash-arith' -- $quiet
#cargo test --package 'yash-builtin' -- $quiet
cargo test --package 'yash-builtin' --no-default-features -- $quiet
#cargo test --package 'yash-cli' -- $quiet
#cargo test --package 'yash-env' -- $quiet
#cargo test --package 'yash-fnmatch' -- $quiet
#cargo test --package 'yash-prompt' -- $quiet
#cargo test --package 'yash-quote' -- $quiet
#cargo test --package 'yash-semantics' -- $quiet
#cargo test --package 'yash-syntax' -- $quiet
cargo test --package 'yash-syntax' --features annotate-snippets -- $quiet

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
