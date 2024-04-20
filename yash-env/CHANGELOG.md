# Changelog

All notable changes to `yash-env` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - Unreleased

### Fixed

- `RealSystem::open_tmpfile` no longer returns a file descriptor with the
  `O_CLOEXEC` flag set.

## [0.1.0] - 2024-04-13

### Added

- Initial implementation of the `yash-env` crate

[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.1.0
