set -Ceu

package=${1:?package name required as first argument}
version=$(cargo metadata --format-version=1 --no-deps --all-features |
    jq -r '.packages[] | select(.name == "'"$package"'") | .version')
today=$(date +%Y-%m-%d)
success=true

grep -Fiq "$package" "$package/README.md" || {
    success=false
    printf 'error: README.md does not mention the package name\n' >&2
}

ls "$package" | grep -q '^LICENSE' || {
    success=false
    printf 'error: no LICENSE file found\n' >&2
}

for changelog in "$package"/CHANGELOG*.md; do
    grep -Fqx "## [$version] - $today" "$changelog" || {
        success=false
        printf 'error: release date for version %s is not set to %s in %s\n' \
            "$version" "$today" "$changelog" >&2
    }
done

"$success" # return the final exit status
