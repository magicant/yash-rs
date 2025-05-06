# Changelog

All notable changes to `yash-prompt` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] - Unreleased

### Changed

- External dependency versions:
    - yash-env 0.7.0 → 0.8.0
    - yash-semantics 0.7.0 → 0.8.0
    - yash-syntax 0.14.0 → 0.15.0

## [0.5.0] - 2025-04-26

### Changed

- External dependency versions:
    - Rust 1.85.0 → 1.86.0
    - yash-env 0.6.0 → 0.7.0
    - yash-semantics 0.6.0 → 0.7.0

## [0.4.0] - 2025-03-23

### Changed

- External dependency versions:
    - Rust 1.82.0 → 1.85.0
    - yash-env 0.5.0 → 0.6.0
    - yash-semantics 0.5.0 → 0.6.0
    - yash-syntax 0.13.0 → 0.14.0

## [0.3.0] - 2024-12-14

### Changed

- External dependency versions:
    - Rust 1.79.0 → 1.82.0
    - yash-env 0.4.0 → 0.5.0
    - yash-syntax 0.12.0 → 0.13.0
- Internal dependency versions:
    - futures-util 0.3.28 → 0.3.31
    - yash-semantics 0.4.0 → 0.5.0

## [0.2.0] - 2024-09-29

### Changed

- `impl yash_syntax::input::Input for Prompter` now conforms to the new
  definition of the `next_line` method.
- External dependency versions:
    - Rust 1.77.0 → 1.79.0
    - yash-env 0.2.0 → 0.4.0
    - yash-syntax 0.10.0 → 0.12.0
- Internal dependency versions:
    - yash-semantics 0.3.0 → 0.4.0

### Removed

- The type parameter constraint `T: 'a` is removed from the declaration of
  `Prompter<'a, 'b, T>`.
- The redundant lifetime constraint `T: 'a` is removed from the implementation
  of `yash_syntax::input::Input` for `Prompter<'a, 'b, T>`.
- Internal dependencies:
    - async-trait 0.1.73

## [0.1.0] - 2024-07-13

### Added

- Initial implementation of the `yash-prompt` crate

[0.6.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.6.0
[0.5.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.5.0
[0.4.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.4.0
[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.1.0
