# Yash-syntax

`yash-syntax` is a Rust library crate for parsing shell script source code.
This crate is part of [yash](../README.md), but can be used independently to
parse POSIX-compatible shell scripts.

Note that `yash-syntax` does not include functionality for executing parsed scripts.

[![yash-syntax at crates.io](https://img.shields.io/crates/v/yash-syntax.svg)](https://crates.io/crates/yash-syntax)
[![yash-syntax at docs.rs](https://docs.rs/yash-syntax/badge.svg)](https://docs.rs/yash-syntax)
[![Build status](https://github.com/magicant/yash-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/magicant/yash-rs/actions/workflows/ci.yml)

- [Changelog](CHANGELOG.md)

## Features

- Parsing POSIX-compatible shell scripts
- Supporting all syntax constructs including compound commands
- Performing alias substitution

## Usage

Add `yash-syntax` as a dependency in your `Cargo.toml`.

See the [API documentation](https://docs.rs/yash-syntax) for details.

<!-- TODO code example -->

## License

This crate is distributed under [GPLv3](LICENSE-GPL).
