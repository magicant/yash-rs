# Yash-fnmatch

`yash-fnmatch` is a Rust library crate for performing glob-style pattern
matching. This crate is part of [yash](../README.md), but can be used
independently.

This crate recognizes all the features of the pattern matching notation as
defined in POSIX. However, this crate does not (yet) support any
locale-dependent behaviors.

[![yash-fnmatch at crates.io](https://img.shields.io/crates/v/yash-fnmatch.svg)](https://crates.io/crates/yash-fnmatch)
[![yash-fnmatch at docs.rs](https://docs.rs/yash-fnmatch/badge.svg)](https://docs.rs/yash-fnmatch)
[![Build status](https://github.com/magicant/yash-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/magicant/yash-rs/actions/workflows/rust.yml)

- [Changelog](CHANGELOG.md)

## Usage

Add `yash-fnmatch` as a dependency in your `Cargo.toml`.

``` rust
use yash_fnmatch::{Pattern, without_escape};
let p = Pattern::parse(without_escape("r*g")).unwrap();
assert_eq!(p.find("string"), Some(2..6));
```

## License

[MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-Apache), at your option

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Similar crates

`yash-fnmatch` is very similar to the
[`fnmatch-regex`](https://crates.io/crates/fnmatch-regex) crate in that both
perform matching by converting the pattern to a regular expression.
`yash-fnmatch` tries to support the POSIX specification as much as possible
rather than introducing unique (non-portable) functionalities.
