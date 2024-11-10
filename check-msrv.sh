set -Ceu

if [ "$*" = "" ]; then quiet='--quiet'; else quiet=''; fi

set -x

# Make sure the files are clean before we modify them
git diff --exit-code -- Cargo.lock Cargo.toml
trap 'git checkout -- Cargo.lock Cargo.toml' EXIT

update_workspace_member() {
    cat >| Cargo.toml <<EOF
[workspace]
members = ["$1"]
resolver = "2"
EOF
}

prepare() {
    update_workspace_member "$1"
    cargo +nightly update -Z direct-minimal-versions
    msrv=$(cargo metadata --format-version=1 |
        jq -r ".packages[] | select(.name == \"$1\") | .rust_version")
}

prepare yash-arith
cargo +$msrv test --package yash-arith -- $quiet

prepare yash-builtin
cargo +$msrv test --package yash-builtin -- $quiet

prepare yash-cli
cargo +$msrv test --package yash-cli -- $quiet

prepare yash-env
cargo +$msrv test --package yash-env -- $quiet

prepare yash-env-test-helper
cargo +$msrv test --package yash-env-test-helper -- $quiet

prepare yash-executor
cargo +$msrv test --package yash-executor -- $quiet

prepare yash-fnmatch
cargo +$msrv test --package yash-fnmatch -- $quiet

prepare yash-prompt
cargo +$msrv test --package yash-prompt -- $quiet

prepare yash-quote
cargo +$msrv test --package yash-quote -- $quiet

prepare yash-semantics
cargo +$msrv test --package yash-semantics -- $quiet

prepare yash-syntax
cargo +$msrv test --package yash-syntax -- $quiet
cargo +$msrv test --package yash-syntax --features annotate-snippets -- $quiet
