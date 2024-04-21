# Changelog

All notable changes to `yash-semantics` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - Unreleased

### Added

- Support for the `ErrExit` shell option in multi-command pipelines

### Changed

- `<expansion::Error as handle::Handle>::handle` now returns `Divert::Exit`
  instead of `Divert::Interrupt` when the `ErrExit` shell option is applicable.

### Fixed

- A `for` loop without any words after `in` now correctly returns an exit status
  of `0` rather than keeping the previous exit status.

## [0.1.0] - 2024-04-13

### Added

- Initial implementation of the `yash-semantics` crate

[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.1.0
