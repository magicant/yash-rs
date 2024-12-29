# Yash-cli

`yash-cli` is a command-line interface for the [yash](../README.md) shell
implementation. This package provides the `yash3` binary, which integrates
many dependencies into a single executable. It is intended to be used as a
standalone shell.

This package also contains the `yash-cli` library crate, which actually
implements the shell. The library is not intended to be used by other
programs.

[![yash-cli at crates.io](https://img.shields.io/crates/v/yash-cli.svg)](https://crates.io/crates/yash-cli)
[![yash-cli at docs.rs](https://docs.rs/yash-cli/badge.svg)](https://docs.rs/yash-cli)
[![Build status](https://github.com/magicant/yash-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/magicant/yash-rs/actions/workflows/ci.yml)

## Versioning

Our semantic versioning policy for this package covers the features of the
shell itself. We will increment the minor version number when adding new
features (new syntax, new command line options, new built-in utilities, etc.),
and the patch version number when fixing bugs. We will increment the major
version number when making incompatible changes to the shell's existing
features.

Note that the `yash-cli` library crate is not intended to be used by other
programs, so we make any changes to it without following semantic versioning.

- [Changelog](CHANGELOG.md)

## License

This crate is distributed under [GPLv3](LICENSE-GPL).
