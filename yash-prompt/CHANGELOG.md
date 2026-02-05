# Changelog

All notable changes to `yash-prompt` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Terminology: A _public dependency_ is one that’s exposed through this crate’s
public API (e.g., re-exported types).
A _private dependency_ is used internally and not visible to downstream users.

## [0.10.1] - Unreleased

### Changed

- Public dependency versions:
    - yash-env 0.12.0 → 0.12.1

## [0.10.0] - 2026-02-04

### Changed

- Public dependency versions:
    - yash-env 0.11.0 → 0.12.0

## [0.9.0] - 2026-01-16

### Changed

- Public dependency versions:
    - Rust 1.86.0 → 1.87.0
    - yash-env 0.10.0 → 0.11.0
- Every type and function now takes a type parameter representing the required
  system interface due to the introduction of the type parameter to `Env` in
  the `yash-env` crate.

## [0.8.0] - 2025-11-26

### Added

- `ExpandText`: Declares the function type for text expansion in prompts.

### Changed

- `expand_posix` now requires an `ExpandText` function injected through the
  environment's `any` storage to perform text expansion.
- Public dependency versions:
    - yash-env 0.9.0 → 0.10.0
- yash-syntax is now a private dependency.
- Private dependency versions:
    - yash-syntax 0.16.0 → 0.17.0

### Removed

- Private dependencies:
    - yash-semantics 0.11.0

## [0.7.1] - 2025-11-07

### Changed

- Private dependency versions:
    - yash-semantics 0.10.0 → 0.11.0

## [0.7.0] - 2025-10-13

### Changed

- Public dependency versions:
    - yash-syntax 0.15.2 → 0.16.0
    - yash-env 0.8.0 → 0.9.0
- Private dependency versions:
    - yash-semantics 0.9.0 → 0.10.0

## [0.6.1] - 2025-09-23

### Changed

- Public dependency versions:
    - yash-syntax 0.15.0 → 0.15.2
- Private dependency versions:
    - yash-semantics 0.8.0 → 0.9.0

## [0.6.0] - 2025-05-11

### Changed

- Public dependency versions:
    - yash-env 0.7.0 → 0.8.0
    - yash-syntax 0.14.0 → 0.15.0
- Private dependency versions:
    - yash-semantics 0.7.0 → 0.8.0

## [0.5.0] - 2025-04-26

### Changed

- Public dependency versions:
    - Rust 1.85.0 → 1.86.0
    - yash-env 0.6.0 → 0.7.0
- Private dependency versions:
    - yash-semantics 0.6.0 → 0.7.0

## [0.4.0] - 2025-03-23

### Changed

- Public dependency versions:
    - Rust 1.82.0 → 1.85.0
    - yash-env 0.5.0 → 0.6.0
    - yash-syntax 0.13.0 → 0.14.0
- Private dependency versions:
    - yash-semantics 0.5.0 → 0.6.0

## [0.3.0] - 2024-12-14

### Changed

- Public dependency versions:
    - Rust 1.79.0 → 1.82.0
    - yash-env 0.4.0 → 0.5.0
    - yash-syntax 0.12.0 → 0.13.0
- Private dependency versions:
    - futures-util 0.3.28 → 0.3.31
    - yash-semantics 0.4.0 → 0.5.0

## [0.2.0] - 2024-09-29

### Changed

- `impl yash_syntax::input::Input for Prompter` now conforms to the new
  definition of the `next_line` method.
- Public dependency versions:
    - Rust 1.77.0 → 1.79.0
    - yash-env 0.2.0 → 0.4.0
    - yash-syntax 0.10.0 → 0.12.0
- Private dependency versions:
    - yash-semantics 0.3.0 → 0.4.0

### Removed

- The type parameter constraint `T: 'a` is removed from the declaration of
  `Prompter<'a, 'b, T>`.
- The redundant lifetime constraint `T: 'a` is removed from the implementation
  of `yash_syntax::input::Input` for `Prompter<'a, 'b, T>`.
- Private dependencies:
    - async-trait 0.1.73

## [0.1.0] - 2024-07-13

### Added

- Initial implementation of the `yash-prompt` crate

[0.10.1]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.10.1
[0.10.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.10.0
[0.9.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.9.0
[0.8.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.8.0
[0.7.1]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.7.1
[0.7.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.7.0
[0.6.1]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.6.1
[0.6.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.6.0
[0.5.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.5.0
[0.4.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.4.0
[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.1.0
