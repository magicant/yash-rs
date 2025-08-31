set -Ceu
unset CDPATH
cd -P -- "$(dirname "$0")"

if [ $# -eq 0 ]; then
    printf 'error: specify at least one package to release\n' >&2
    exit 2
fi

# Ensure that the working directory is clean
if ! git diff --quiet HEAD; then
    printf 'error: working directory is not clean\n' >&2
    exit 1
fi

# Ensure that the packages are not already released
for package do
    version=$(cargo metadata --format-version=1 --no-deps --all-features |
        jq -r '.packages[] | select(.name == "'"$package"'") | .version')
    if curl --fail --location --silent \
        "https://crates.io/api/v1/crates/$package/$version" >/dev/null; then
        printf 'error: %s %s is already released\n' "$package" "$version" >&2
        printf 'bump the version in Cargo.toml and try again\n' >&2
        exit 1
    fi
done

# Ensure that all required updates are included in the release
# (For example, if yash-env is included in $@, yash-env depends on yash-syntax,
# and the version of yash-syntax specified in the Cargo.toml of yash-env is not
# yet released, we need to release yash-syntax first. If yash-syntax is not
# included in $@, we bail out.)
## List all the direct dependencies of the packages being released
for package do
    cargo metadata --format-version=1 --no-deps --all-features |
        jq -r '.packages[] | select(.name == "'"$package"'") | .dependencies[] | select(.source == null) | "\(.name) \(.req)"'
done |
## Remove duplicates
sort -u |
## Check each dependency
while read -r dependency version; do
    ## If we're releasing the dependency as well, that's fine
    case " $* " in
        (*" $dependency "*)
            continue
            ;;
    esac

    ## Trim the '^' from the version specification
    version=${version#^}
    
    ## Check if the dependency is already released
    if curl --fail --location --silent --show-error \
        "https://crates.io/api/v1/crates/$dependency/$version" >/dev/null; then
        continue
    fi

    printf 'error: %s %s is not released yet\n' "$dependency" "$version" >&2
    printf 'add it to the list of packages to release and try again\n' >&2
    exit 1
done

# Write the current date in CHANGELOG.md
today=$(date +%Y-%m-%d)
for package do
    version=$(cargo metadata --format-version=1 --no-deps --all-features |
        jq -r '.packages[] | select(.name == "'"$package"'") | .version')
    sed -e '/^## \['"$version"'\] - Unreleased$/s/Unreleased/'"$today"'/' \
        "$package/CHANGELOG.md" > "$package/CHANGELOG.md.tmp"
    mv "$package/CHANGELOG.md.tmp" "$package/CHANGELOG.md"
done

# Remove the `publish = false` line from Cargo.toml
for package do
    sed '/^publish = false$/d' "$package/Cargo.toml" > "$package/Cargo.toml.tmp"
    mv "$package/Cargo.toml.tmp" "$package/Cargo.toml"
done

# Stop if there are any changes to be committed
if ! git diff --exit-code --name-status HEAD; then
    printf 'confirm and commit the changes to the above files and try again\n' >&2
    exit 1
fi

# Check if the packages are ready for release
./check.sh
./check-extra.sh
for package do
    ./check-release.sh "$package"
done

# Confirm the release
{
printf 'Are you sure you want to release the following packages?\n'
printf '  %s\n' "$@"
printf 'This will publish the packages to crates.io.\n'
printf 'Type "release" to proceed: '
} >&2
read -r confirm
if [ "$confirm" != "release" ]; then
    printf 'Aborting release\n' >&2
    exit 1
fi

# Release the packages
# (We need to do this in topological order of dependencies)
while [ $# -gt 0 ]; do
    package="$1"
    shift

    # If the package has dependencies that are not yet released, skip it
    # so that we can release them first.
    for dependency in $(cargo metadata --format-version=1 --no-deps --all-features |
        jq -r '.packages[] | select(.name == "'"$package"'") | .dependencies[] | select(.source == null) | .name')
    do
        case " $* " in
            (*" $dependency "*)
                # Re-add the package to the tail of the list
                # so that we can try to release it later
                # after its dependencies have been released.
                set -- "$@" "$package"
                continue 2
                ;;
        esac
    done

    cargo publish --package "$package"

    version=$(cargo metadata --format-version=1 --no-deps --all-features |
        jq -r '.packages[] | select(.name == "'"$package"'") | .version')
    git tag --sign --message "Release $package $version" "$package-$version"
    git push origin "tags/$package-$version"
done
