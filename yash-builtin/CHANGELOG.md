# Changelog

All notable changes to `yash-builtin` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - Unreleased

### Added

- `impl Default for common::syntax::OptionSpec`
- `trap::CondSpec`
- `trap::Error`
- `trap::ErrorCause`

### Changed

- `trap::Command::execute` now returns `Result<String, Vec<Error>>`
  (previously `Result<String, Vec<(SetActionError, Condition, Field)>>`).

## [0.1.0] - 2024-04-13

### Added

- Initial implementation of the `yash-builtin` crate

[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.1.0
