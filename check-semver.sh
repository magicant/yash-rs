set -Ceux

# Make sure next releases have correct semantic versions.
cargo semver-checks --package 'yash-arith'
cargo semver-checks --package 'yash-builtin'
cargo semver-checks --package 'yash-env'
cargo semver-checks --package 'yash-env-test-helper'
cargo semver-checks --package 'yash-executor'
cargo semver-checks --package 'yash-fnmatch'
cargo semver-checks --package 'yash-prompt'
cargo semver-checks --package 'yash-quote'
cargo semver-checks --package 'yash-semantics'
cargo semver-checks --package 'yash-syntax'
