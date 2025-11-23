set -Ceu

if [ "$*" = "" ]; then quiet='--quiet'; else quiet=''; fi

set -x

# Make sure the files are clean before we modify them
git diff --exit-code -- Cargo.lock Cargo.toml
trap 'git checkout -- Cargo.lock Cargo.toml' EXIT

# $1 = package name
# $2, $3, ... = additional options to `cargo test`
check() {
    package="$1"
    shift
    if [ "$#" -eq 0 ]; then
        set ''
    fi

    sed -e '/^members = / s;".*";"'"$package"'";' Cargo.toml >| Cargo.toml.tmp
    mv Cargo.toml.tmp Cargo.toml
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
check yash-syntax
