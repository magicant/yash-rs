# Yash-quote

`yash-quote` is a Rust library crate for quoting strings used in a POSIX shell script.
This crate provides just one function: `quote`. It returns a quoted version of the argument string.

[![yash-quote at crates.io](https://img.shields.io/crates/v/yash-quote.svg)](https://crates.io/crates/yash-quote)
[![yash-quote at docs.rs](https://img.shields.io/docsrs/yash-quote/latest)](https://docs.rs/yash-quote)
[![Build status](https://github.com/magicant/yash-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/magicant/yash-rs/actions/workflows/ci.yml)

- [Changelog](CHANGELOG.md)

## Usage

Add `yash-quote` as a dependency in your `Cargo.toml`.

``` rust
use std::borrow::Cow::{Borrowed, Owned};
use yash_quote::quote;
assert_eq!(quote("foo"), Borrowed("foo"));
assert_eq!(quote(""), Owned::<str>("''".to_owned()));
assert_eq!(quote("$foo"), Owned::<str>("'$foo'".to_owned()));
assert_eq!(quote("'$foo'"), Owned::<str>(r#""'\$foo'""#.to_owned()));
```

## License

[MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-Apache), at your option

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Similar crates

- [`r-shquote`](https://crates.io/crates/r-shquote) provides a function that always quotes using single quotes.
- The `quote` function of the [`shell_words`](https://crates.io/crates/shell-words) crate is similar but tries to return the argument unchanged if possible. Unlike `yash-quote`, it only supports ASCII characters.
- [`snailquote`](https://crates.io/crates/snailquote) is also similar but uses an original format that is not fully compatible with POSIX shells.
- [`shell_quote`](https://crates.io/crates/shell-quote) returns a string escaped using Bash's `$'...'` notation.

For the reverse operation of `quote`, the [`yash-syntax`](../yash-syntax) crate provides the `unquote` function.
