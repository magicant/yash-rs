# Yash-executor

`yash-executor` is a Rust library that provides a simple executor for running
futures. It is designed to be used in single-threaded applications where you
want to run futures concurrently, but you don't need to run them on multiple
threads.

This crate is free of locks and atomic operations at the cost of unsafe
spawning. Wakers used in this crate are thread-unsafe and not guarded by locks
or atomics, so you must ensure that wakers are not shared between threads.

[![yash-executor at crates.io](https://img.shields.io/crates/v/yash-executor.svg)](https://crates.io/crates/yash-executor)
[![yash-executor at docs.rs](https://docs.rs/yash-executor/badge.svg)](https://docs.rs/yash-executor)
[![Build status](https://github.com/magicant/yash-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/magicant/yash-rs/actions/workflows/ci.yml)

- [Changelog](CHANGELOG.md)

## License

[MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-Apache), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Similar crates

The [`futures-executor`] crate's `LocalPool` is similar but rejects reentrant
calls to `run`, etc. as it depends on thread-local states. The `yash-executor`
crate allows creating and running multiple executors that work independently, so
users can have more fine-grained control over how concurrent tasks are run.

The [`async-executor`] crate provides `LocalExecutor` which is a wrapper around
the thread-safe `Executor` and depends on locks for synchronization.
The `yash-executor` crate is lock-free at the cost of unsafe spawning.

The [`simple-async-local-executor`] crate is similar to `yash-executor` but
also provides event signaling. Its implementation is free of locks and atomics
but still safe because the `Waker` is a dummy and the `Executor` polls all
futures even if not awakened.

[`futures-executor`]: https://crates.io/crates/futures-executor
[`async-executor`]: https://crates.io/crates/async-executor
[`simple-async-local-executor`]: https://crates.io/crates/simple-async-local-executor
