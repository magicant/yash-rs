set -Ceu

if [ "$*" = "" ]; then quiet='--quiet'; else quiet=''; fi

set -x

cargo check

cargo tomlfmt --dryrun --path Cargo.toml
cargo tomlfmt --dryrun --path yash/Cargo.toml
cargo tomlfmt --dryrun --path yash-arith/Cargo.toml
cargo tomlfmt --dryrun --path yash-builtin/Cargo.toml
cargo tomlfmt --dryrun --path yash-env/Cargo.toml
cargo tomlfmt --dryrun --path yash-fnmatch/Cargo.toml
cargo tomlfmt --dryrun --path yash-quote/Cargo.toml
cargo tomlfmt --dryrun --path yash-semantics/Cargo.toml
cargo tomlfmt --dryrun --path yash-syntax/Cargo.toml

cargo test --package 'yash-arith' -- $quiet
cargo test --package 'yash-builtin' -- $quiet
cargo test --package 'yash-builtin' --no-default-features -- $quiet
cargo test --package 'yash-env' -- $quiet
cargo test --package 'yash-fnmatch' -- $quiet
cargo test --package 'yash-quote' -- $quiet
cargo test --package 'yash-semantics' -- $quiet
cargo test --package 'yash-syntax' -- $quiet
cargo test --package 'yash-syntax' --features annotate-snippets -- $quiet
