set -Ceu

package=${1:?package name required as first argument}
version=$(
    grep '^version *=' "$package/Cargo.toml" |
    sed -e 's/.*"\(.*\)".*/\1/'
)
today=$(date +%Y-%m-%d)
success=true

cargo semver-checks -p "$package" || success=false

grep -Fiq "$package" "$package/README.md" || {
    success=false
    printf 'error: README.md does not mention the package name\n' >&2
}

ls "$package" | grep -q '^LICENSE' || {
    success=false
    printf 'error: no LICENSE file found\n' >&2
}

grep -Fqx "## [$version] - $today" "$package/CHANGELOG.md" || {
    success=false
    printf 'error: release date for version %s is not set to %s in CHANGELOG.md\n' "$version" "$today" >&2
}

"$success" # return the final exit status
