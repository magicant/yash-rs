# Yash-arith

`yash-arith` is a Rust library crate for performing the POSIX shell's arithmetic
expansion. This crate is part of [yash](../README.md) but can be used
independently.

[![yash-arith at crates.io](https://img.shields.io/crates/v/yash-arith.svg)](https://crates.io/crates/yash-arith)
[![yash-arith at docs.rs](https://docs.rs/yash-arith/badge.svg)](https://docs.rs/yash-arith)
[![Build status](https://github.com/magicant/yash-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/magicant/yash-rs/actions/workflows/rust.yml)


- [Changelog](CHANGELOG.md)

## Usage

Add `yash-arith` as a dependency in your `Cargo.toml`.

``` rust
use std::collections::HashMap;
use yash_arith::{eval, Value};
let mut env = HashMap::new();
env.insert("a".to_owned(), "2".to_owned());
let result = eval("1 + a", &mut env);
assert_eq!(result, Ok(Value::Integer(3)));
```

## License

This crate is distributed under [GPLv3](LICENSE-GPL).
