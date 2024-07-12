# Changelog

All notable changes to `yash-builtin` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - Unreleased

### Changed

- External dependency versions:
    - Rust 1.75.0 → 1.77.0
    - yash-syntax 0.9.0 → 0.10.0

### Fixed

- The read built-in now shows a prompt when reading a continued line.
- The source built-in now echoes the input when the verbose shell option is set.
- The break and continue built-ins now return `ExitStatus::ERROR` for syntax
  errors and `ExitStatus::FAILURE` for semantic errors. Previously, they always
  returned `ExitStatus::ERROR` for both types of errors, while the documentation
  stated that they returned `ExitStatus::FAILURE` for semantic errors.
- The set built-in no longer enables stopper handlers (see
  `yash_env::trap::TrapSet`) when invoked with the `-m` option in a subshell of
  an interactive shell. Previously, it enabled stopper handlers in such cases,
  which was inconsistent with the job control behavior implemented in the
  `yash-semantics` crate.

## [0.2.0] - 2024-06-09

### Added

- `impl Default for common::syntax::OptionSpec`
- `kill::Signal`
- `kill::syntax::parse_signal`
- `kill::print::InvalidSignal`
- Support for real-time signals in the kill built-in
- `trap::CondSpec`
- `trap::Error`
- `trap::ErrorCause`

### Changed

- External dependency versions
    - yash-env 0.1.0 → 0.2.0
    - yash-semantics 0.1.0 → 0.2.0
    - yash-syntax 0.8.0 → 0.9.0
- `kill::syntax::parse_signal` now returns an `Option<kill::Signal>` instead of
  `Option<Option<yash_env::trap::Signal>>`
- `kill::send::execute` now additionally takes the
  `signal_origin: Option<&Field>` argument.
- `kill::print::print` now additionally takes the `system: &SystemEx` argument
  and returns `Result<String, Vec<InvalidSignal>>` (previously `String`).
- `kill::Command::Send::signal` is now a `kill::Signal`
  (previously `Option<yash_env::trap::Signal>`).
- `kill::Command::Send` now has a `signal_origin: Option<Field>` field.
- `kill::Command::Print::signals` is now a `Vec<kill::Signal>`
  (previously `Vec<yash_env::trap::Signal>`).
- `trap::Command::SetAction::conditions` is now a `Vec<(CondSpec, Field)>`
  (previously `Vec<(Condition, Field)>`).
- `trap::Command::execute` now returns `Result<String, Vec<Error>>`
  (previously `Result<String, Vec<(SetActionError, Condition, Field)>>`).
- `trap::display_traps` is now marked `#[must_use]`.
- `trap::display_traps` is now additionally takes a
  `yash_env::trap::SignalSystem` argument.
- `wait::core::Error::Trapped` now contains a `yash_env::signal::Number`
  instead of a `yash_env::trap::Signal`.

### Removed

- `kill::syntax::parse_signal_name`
- `kill::syntax::parse_signal_name_or_number`

## [0.1.0] - 2024-04-13

### Added

- Initial implementation of the `yash-builtin` crate

[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.1.0
