set -Ceu

if [ "$*" = "" ]; then quiet='--quiet'; else quiet=''; fi

set -x

cargo fmt -- --check
cargo test -- $quiet
cargo doc
cargo clippy --all-targets
