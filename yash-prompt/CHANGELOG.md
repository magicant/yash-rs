# Changelog

All notable changes to `yash-prompt` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - Unreleased

### Changed

- External dependency versions:
    - Rust 1.77.0 → 1.79.0
    - yash-env 0.2.0 → 0.3.0
    - yash-syntax 0.10.0 → 0.11.0

### Removed

- The type parameter constraint `T: 'a` is removed from the declaration of
  `Prompter<'a, 'b, T>`.
- The redundant lifetime constraint `T: 'a` is removed from the implementation
  of `yash_syntax::input::Input` for `Prompter<'a, 'b, T>`.

## [0.1.0] - 2024-07-13

### Added

- Initial implementation of the `yash-prompt` crate

[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-prompt-0.1.0
