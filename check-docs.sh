set -Ceux

mdbook build docs
mdbook test docs
docs/doctest.sh
