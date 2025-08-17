set -Ceux

mdbook build docs

# Skip Rust doctest since we have no Rust code examples
# mdbook test docs

docs/doctest.sh
docs/indexsort.sh
