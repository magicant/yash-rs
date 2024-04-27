# Changelog

All notable changes to `yash-semantics` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - Unreleased

### Added

- Support for the `ErrExit` shell option in multi-command pipelines

### Changed

- In the `expansion::initial` module:
    - `ValueState` was renamed to `Vacancy`.
    - `EmptyError` was renamed to `VacantError`.
    - `EmptyError::state` was renamed to `VacantError::vacancy`.
- `expansion::ErrorCause::EmptyExpansion` was renamed to `expansion::ErrorCause::VacantExpansion`.
- `<expansion::Error as handle::Handle>::handle` now returns `Divert::Exit`
  instead of `Divert::Interrupt` when the `ErrExit` shell option is applicable.
- `expansion::glob::glob` no longer requires search permission for the parent
  directory of the last pathname component in the pattern when the last
  component contains a pattern character.

### Fixed

- A `for` loop without any words after `in` now correctly returns an exit status
  of `0` rather than keeping the previous exit status.
- `<TextUnit as Expand>::expand` now correctly expands an unset parameter with a
  switch to an empty string regardless of the `Unset` shell option. Previously,
  it would expand to an empty string only if the `Unset` shell option was on.
- The parameter expansion of an unset variable with a `Length` modifier now
  correctly expands to `0` rather than an empty string.
- `expansion::glob::glob` now handles backslash escapes in glob patterns
  correctly.
- `trap::run_traps_for_caught_signals`, `trap::run_trap_if_caught`, and
  `trap::run_exit_trap` now propagate the exit status of the executed trap
  action if it is interrupted by a shell error raising `Divert::Interrupt(_)`.
- `trap::run_exit_trap` is now called on the exit of a subshell that is running
  a command substitution, an asynchronous and-or list, or a job-controlled
  pipeline.

## [0.1.0] - 2024-04-13

### Added

- Initial implementation of the `yash-semantics` crate

[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.1.0
