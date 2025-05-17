# Installation

## Downloading precompiled binaries

Precompiled binaries are available for the following platforms:

- aarch64-unknown-linux-gnu
- x86_64-unknown-linux-gnu
- aarch64-apple-darwin
- x86_64-apple-darwin

You can download the latest release from the [GitHub releases page](https://github.com/magicant/yash-rs/releases).
Download the appropriate binary for your platform and place it in a directory included in your `PATH` environment variable.

## Building from source

Yash is written in Rust, so you need to have the Rust toolchain installed.
The recommended way to install Rust is via [rustup](https://rustup.rs/).

If you are using Windows Subsystem for Linux (WSL), make sure to install the Linux version of rustup, not the native Windows version.
For alternative installation methods, refer to [the rustup book](https://rust-lang.github.io/rustup/installation/other.html).

By default, installing rustup also installs the stable Rust toolchain.
If the stable toolchain is not installed, you can add it with the following command:

```bash
rustup default stable
```

To install yash, run:

```bash
cargo install yash-cli
```

## Running yash

After installation, you can run `yash3` from the command line.
