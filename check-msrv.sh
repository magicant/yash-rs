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

# $1 = package name
# $2, $3, ... = additional options to `cargo test`
check() {
    package="$1"
    shift
    if [ "$#" -eq 0 ]; then
        set ''
    fi

    update_workspace_member "$package"
    cargo +nightly update -Z direct-minimal-versions
    msrv=$(cargo metadata --format-version=1 |
        jq -r ".packages[] | select(.name == \"$package\") | .rust_version")

    for options do
        cargo +$msrv test --package "$package" $options -- $quiet
    done
}

check yash-arith
check yash-builtin
check yash-cli
check yash-env
check yash-env-test-helper
check yash-executor
check yash-fnmatch
check yash-prompt
check yash-quote
check yash-semantics
check yash-syntax '' '--features annotate-snippets'
