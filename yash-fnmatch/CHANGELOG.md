# Changelog

All notable changes to `yash-fnmatch` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.1] - Unreleased

### Changed

- Dependency versions
    - Rust 1.58.0 → 1.60.0
    - regex 1.5.6 → 1.8.1
    - regex-syntax 0.6.26 → 0.7.1
    - thiserror 1.0.31 → 1.0.38

## [1.1.0] - 2022-10-22

### Added

- `Pattern::rfind`

## [1.0.1] - 2022-10-01

### Fixed

- A bug where an `Ast` containing an empty collating symbol or equivalence class
  in a `BracketItem::Atom` was producing a malformed regex rather than returning
  an `Error::EmptyCollatingSymbol`.

## [1.0.0] - 2022-07-02

The first release.

### Added

- Fundamental items for pattern matching
    - `Pattern`, `Config`, `Error`
    - `PatternChar`, `with_escape`, `without_escape`
    - `ast`
        - `Ast`, `Atom`, `Bracket`, `BracketItem`, `BracketAtom`

[1.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-fnmatch-1.1.0
[1.0.1]: https://github.com/magicant/yash-rs/releases/tag/yash-fnmatch-1.0.1
[1.0.0]: https://github.com/magicant/yash-rs/releases/tag/yash-fnmatch-1.0.0
