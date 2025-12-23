# Changelog

All notable changes to `yash-fnmatch` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Terminology: A _public dependency_ is one that’s exposed through this crate’s
public API (e.g., re-exported types).
A _private dependency_ is used internally and not visible to downstream users.

## [1.1.3] - Unreleased

### Changed

- Public dependency versions
    - Rust 1.65.0 → 1.87.0

## [1.1.2] - 2024-12-14

### Changed

- Private dependency versions
    - thiserror 1.0.47 → 2.0.4

## [1.1.1] - 2024-04-10

### Changed

- Public dependency versions
    - Rust 1.58.0 → 1.65.0
- Private dependency versions
    - regex 1.5.6 → 1.9.4
    - regex-syntax 0.6.26 → 0.8.2
    - thiserror 1.0.31 → 1.0.47

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

[1.1.3]: https://github.com/magicant/yash-rs/releases/tag/yash-fnmatch-1.1.3
[1.1.2]: https://github.com/magicant/yash-rs/releases/tag/yash-fnmatch-1.1.2
[1.1.1]: https://github.com/magicant/yash-rs/releases/tag/yash-fnmatch-1.1.1
[1.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-fnmatch-1.1.0
[1.0.1]: https://github.com/magicant/yash-rs/releases/tag/yash-fnmatch-1.0.1
[1.0.0]: https://github.com/magicant/yash-rs/releases/tag/yash-fnmatch-1.0.0
