set -Ceux

# Make sure next releases have correct semantic versions.

# The baseline is the latest published version, which the min-publish-age in
# .cargo/config.toml would reject for a while after each release.
export CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=allow

cargo semver-checks --package 'yash-arith'
cargo semver-checks --package 'yash-builtin'
cargo semver-checks --package 'yash-env'
cargo semver-checks --package 'yash-executor'
cargo semver-checks --package 'yash-fnmatch'
cargo semver-checks --package 'yash-prompt'
cargo semver-checks --package 'yash-quote'
cargo semver-checks --package 'yash-semantics'
cargo semver-checks --package 'yash-syntax'
