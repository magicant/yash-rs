set -Ceux

# Make sure the crates can be built with all combinations of features.
# We're performing release checks here to make sure that the release build
# doesn't break due to any missing dependencies or similar issues.
# Other check scripts only perform debug checks, so they won't catch such issues.
cargo check --release --package 'yash-arith' --all-targets
cargo check --release --package 'yash-builtin' --all-targets --no-default-features
cargo check --release --package 'yash-builtin' --all-targets --no-default-features --features yash-semantics
cargo check --release --package 'yash-cli' --all-targets
cargo check --release --package 'yash-env' --all-targets --no-default-features
cargo check --release --package 'yash-env' --all-targets --no-default-features --features test-helper
cargo check --release --package 'yash-env' --all-targets --no-default-features --features yash-executor
cargo check --release --package 'yash-executor' --all-targets
cargo check --release --package 'yash-fnmatch' --all-targets
cargo check --release --package 'yash-prompt' --all-targets
cargo check --release --package 'yash-quote' --all-targets
cargo check --release --package 'yash-semantics' --all-targets
cargo check --release --package 'yash-syntax' --all-targets
