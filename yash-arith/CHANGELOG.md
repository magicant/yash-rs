# Changelog

All notable changes to `yash-arith` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Terminology: A _public dependency_ is one that’s exposed through this crate’s
public API (e.g., re-exported types).
A _private dependency_ is used internally and not visible to downstream users.

## [0.2.3] - Unreleased

### Changed

- Public dependency versions
    - Rust 1.65.0 → 1.87.0

## [0.2.2] - 2024-12-14

### Changed

- Public dependency versions
    - Rust 1.58.0 → 1.65.0
- Private dependency versions
    - thiserror 1.0.47 → 2.0.4

## [0.2.1] - 2023-11-12

### Changed

- Improved documentation

## [0.2.0] - 2023-09-10

### Added

- `TokenError`, `SyntaxError`, `EvalError`, `ErrorCause`, and `Error` now
  implement `std::error::Error`.

### Changed

- Variable access is now fallible.
    - Added associated type `GetVariableError` to `Env`.
    - Changed the return type of `Env::get_variable` from `Option<&str>` to
      `Result<Option<&str>, GetVariableError>`.
    - `Error`, `EvalError` and `ErrorCause` now take two type parameters, the
      first of which is for `GetVariableError`.
    - Changed the return type of `eval` from
      `Result<Value, Error<E::AssignVariableError>>` to
      `Result<Value, Error<E::GetVariableError, E::AssignVariableError>>`.
- Private dependency versions
    - thiserror 1.0.43 → 1.0.47

## [0.1.0] - 2022-10-01

### Added

- Fundamental items for performing arithmetic expansion

[0.2.3]: https://github.com/magicant/yash-rs/releases/tag/yash-arith-0.2.3
[0.2.2]: https://github.com/magicant/yash-rs/releases/tag/yash-arith-0.2.2
[0.2.1]: https://github.com/magicant/yash-rs/releases/tag/yash-arith-0.2.1
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-arith-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-arith-0.1.0
