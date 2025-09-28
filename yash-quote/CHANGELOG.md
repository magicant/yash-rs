# Changelog

All notable changes to `yash-quote` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Terminology: A _public dependency_ is one that’s exposed through this crate’s
public API (e.g., re-exported types).
A _private dependency_ is used internally and not visible to downstream users.

## [1.1.2] - Unreleased

### Changed

- Public dependency versions
    - Rust 1.56.0 → 1.85.0

## [1.1.1] - 2023-11-12

### Changed

- More functions are now `#[inline]` and `#[must_use]`.

## [1.1.0] - 2023-05-01

### Added

- The `Quoted` struct
    - The `as_raw` and `needs_quoting` methods
    - `impl std::fmt::Display for Quoted<'_>`
    - `impl<'a> From<&'a str> for Quoted<'a>`
    - `impl<'a> From<Quoted<'a>> for Cow<'a, str>`
- The `quoted` function

## [1.0.1] - 2023-01-05

### Fixed

- `yash-quote` now quotes strings containing `:~` so that the results can safely
  be used in the value of an assignment.

## [1.0.0] - 2022-02-03

No changes. Just bumping the version number to make the public API stable.

## [0.1.0] - 2021-12-11

### Added

- The `quote` function

[1.1.2]: https://github.com/magicant/yash-rs/releases/tag/yash-quote-1.1.2
[1.1.1]: https://github.com/magicant/yash-rs/releases/tag/yash-quote-1.1.1
[1.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-quote-1.1.0
[1.0.1]: https://github.com/magicant/yash-rs/releases/tag/yash-quote-1.0.1
[1.0.0]: https://github.com/magicant/yash-rs/releases/tag/yash-quote-1.0.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-quote-0.1.0
