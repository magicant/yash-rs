set -Ceux

# Make sure the crates can be built with all combinations of features.
# We're performing release builds here to make sure that the release build
# doesn't break due to any missing dependencies or similar issues.
# Other check scripts only perform debug builds, so they won't catch such issues.
cargo build --release --package 'yash-arith' --all-targets
cargo build --release --package 'yash-builtin' --all-targets --no-default-features
cargo build --release --package 'yash-builtin' --all-targets --no-default-features --features yash-semantics
cargo build --release --package 'yash-cli' --all-targets
cargo build --release --package 'yash-env' --all-targets --no-default-features
cargo build --release --package 'yash-env' --all-targets --no-default-features --features test-helper
cargo build --release --package 'yash-env' --all-targets --no-default-features --features yash-executor
cargo build --release --package 'yash-env-test-helper' --all-targets
cargo build --release --package 'yash-executor' --all-targets
cargo build --release --package 'yash-fnmatch' --all-targets
cargo build --release --package 'yash-prompt' --all-targets
cargo build --release --package 'yash-quote' --all-targets
cargo build --release --package 'yash-semantics' --all-targets
cargo build --release --package 'yash-syntax' --all-targets
