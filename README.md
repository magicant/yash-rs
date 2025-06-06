# Yash-rs

This is a reimplementation project of [Yet Another Shell (yash)](https://magicant.github.io/yash/) in Rust.
Currently, only a minimal subset of the original yash is implemented.
It will be extended to cover more features in the future.

[![Build status](https://github.com/magicant/yash-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/magicant/yash-rs/actions/workflows/ci.yml)

## Features

Currently, yash can run shell scripts written in POSIX-compatible syntax.
Support for interactive shell features is limited.

- [x] Running shell scripts that only use POSIX-compatible syntax and features
- [x] Minimal interactive shell features
- [ ] Enhanced shell features (Extensions to POSIX shell)
- [ ] Performance optimization
- [ ] Locale support

## Supported platforms

Yash should work on any Unix-like system.
We are testing it on Linux and macOS.
Windows is not supported, but it works under the Windows Subsystem for Linux (WSL).

## Installation

To build and install yash, you need to have Rust installed.
Go to <https://rustup.rs/> and follow the instructions to install Rust.
You will need the latest stable version of the Rust compiler.

Make sure the `cargo` tool installed by `rustup` is in your `PATH`.
Then, run the following command:

```sh
cargo install yash-cli
```

## Usage

To run a shell script, run `yash3` with the script file as an argument.
Without an argument, `yash3` will start a read-eval loop, but interactive features are not yet implemented.

The user manual is not yet available, but you can refer to the original yash manual at <https://magicant.github.io/yash/doc/>.

## How to contribute

TBD

## License

Yash is distributed under [GPLv3](yash-cli/LICENSE-GPL).

Exceptionally, you can reuse the `yash-executor`, `yash-fnmatch` and
`yash-quote` crates in your software under the
[MIT License](yash-quote/LICENSE-MIT) or
[Apache License 2.0](yash-quote/LICENSE-Apache), whichever at your option.
