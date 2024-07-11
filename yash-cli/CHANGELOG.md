# Changelog

All notable changes to `yash-cli` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0-beta.2] - Unreleased

### Added

- Internal dependencies:
    - yash-prompt 0.1.0

### Changed

- External dependency versions:
    - Rust 1.75.0 → 1.77.0
- The shell now shows the prompt before reading the input in the interactive mode.
  To achieve this, the `startup::prepare_input` function now applies the
  `yash_prompt::Prompter` decorator to the returned source input.
- The first argument to `startup::prepare_input` is now `env: &'a RefCell<&mut Env>`
  instead of `system: &mut SharedSystem`. This change is to allow the function to
  construct `yash_env::input::Echo` for the returned source input.

### Removed

- `startup::SourceInput::verbose`
    - The caller of `startup::prepare_input` is no longer responsible for setting
      the `verbose` flag of the read-eval loop. The behavior of the verbose option
      is now implemented in `yash_env::input::Echo`, which is included in
      the `startup::SourceInput::input` field.

### Fixed

- The break and continue built-ins no longer allow exiting a trap.
- The read built-in now shows a prompt when reading a continued line.
- The source built-in now echoes the input when the verbose shell option is set.

## [0.1.0-beta.1] - 2024-06-09

### Changed

- External dependency versions
    - yash-builtin 0.1.0 → 0.2.0
    - yash-env 0.1.0 → 0.2.0
    - yash-semantics 0.1.0 → 0.2.0
    - yash-syntax 0.8.0 → 0.9.0
- The shell now enables blocking reads on the standard input if it is a terminal
  or a pipe as [required by POSIX](https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/utilities/sh.html#tag_20_117_06).

## [0.1.0-alpha.1] - 2024-04-13

### Added

- Initial implementation of the `yash-cli` crate

[0.1.0-beta.2]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-beta.1
[0.1.0-alpha.1]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-alpha.1
