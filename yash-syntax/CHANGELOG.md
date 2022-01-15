# Changelog

All notable changes to `yash-syntax` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - ????-??-??

### Changed

- Items in the `source` module:
    - `Line` renamed to `Code`
    - `Location`'s field `line` renamed to `code`
    - `Annotation`'s field `location` replaced with `code` and `column`
- Dependency versions
    - `async-trait` 0.1.50 → 0.1.52
    - `futures-util` 0.3.18 → 0.3.19
    - `itertools` 0.10.1 → 0.10.3

## [0.1.0] - 2021-12-11

### Added

- Functionalities to parse POSIX shell scripts
- Alias substitution support

[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.1.0
