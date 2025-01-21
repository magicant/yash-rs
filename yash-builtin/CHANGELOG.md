# Changelog

All notable changes to `yash-builtin` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] - Unreleased

### Added

The `cd` built-in now supports the `-e` (`--ensure-pwd`) option, which ensures
that the `$PWD` variable is set to the actual current working directory after
changing the working directory. The following items have been added to implement
this feature:

- `common::report`, `common::report_simple`
    - These functions are generalizations of the existing `report_failure`,
      `report_error`, `report_simple_failure`, and `report_simple_error`
      functions that allow returning a custom exit status.
- `cd::EXIT_STATUS_SUCCESS`, `cd::EXIT_STATUS_STALE_PWD`,
  `cd::EXIT_STATUS_CHDIR_ERROR`, `cd::EXIT_STATUS_UNSET_VARIABLE`, and
  `cd::EXIT_STATUS_SYNTAX_ERROR`
    - These constants represent exit statuses that can be returned by the `cd`
      built-in.
- `cd::Command::ensure_pwd`
    - This field represents the new `-e` option of the `cd` built-in.
- `cd::syntax::Error::EnsurePwdNotPhysical`
    - This error variant represents a syntax error that occurs when the `-e`
      option is specified without the `-P` option.

### Changed

- The `cd::chdir::report_failure` function now returns a result with
  `EXIT_STATUS_CHDIR_ERROR`.
- External dependency versions:
    - yash-env 0.5.0 → 0.6.0
    - yash-semantics 0.5.0 → 0.6.0 (optional)
    - yash-syntax 0.13.0 → 0.14.0
- Internal dependency versions:
    - yash-prompt 0.3.0 → 0.4.0 (optional)

## [0.5.0] - 2024-12-14

### Changed

- External dependency versions:
    - yash-env 0.4.0 → 0.5.0
    - yash-semantics 0.4.0 → 0.5.0 (optional)
    - yash-syntax 0.12.0 → 0.13.0
- Internal dependency versions:
    - yash-prompt 0.2.0 → 0.3.0 (optional)

## [0.4.1] - 2024-12-14

### Changed

- The `bg` built-in now updates the `!` special parameter (which is backed by
  `yash_env::job::JobList::last_async_pid`) to the process ID of the background
  job, as required by POSIX.1-2024.
- The `exec` built-in no longer exits the shell when the specified command is
  not found in an interactive shell, as required by POSIX.1-2024.
- External dependency versions:
    - Rust 1.79.0 → 1.82.0
- Internal dependency versions
    - thiserror 1.0.47 → 2.0.4

## [0.4.0] - 2024-09-29

### Added

- This crate now builds on non-Unix platforms.

### Changed

- All APIs that handle `std::path::Path` and `std::path::PathBuf` now use
  `yash_env::path::Path` and `yash_env::path::PathBuf` instead.
    - `cd::assign::new_pwd`
    - `cd::assign::set_pwd`
    - `cd::canonicalize::NonExistingDirectoryError::missing`
    - `cd::canonicalize::canonicalize`
    - `cd::cdpath::search`
    - `cd::chdir::chdir`
    - `cd::chdir::failure_message`
    - `cd::chdir::report_failure`
    - `cd::print::print_path`
    - `cd::shorten::shorten`
    - `cd::target::TargetError::NonExistingDirectory::missing`
    - `cd::target::TargetError::NonExistingDirectory::target`
    - `cd::target::target`
- The `trap::Command::execute` method now allows modifying the trap for signals
  that were ignored on the shell startup if the shell is interactive.
- The `ulimit::Error::Unknown` variant now contains a `yash_env::system::Errno`
  instead of a `std::io::Error`.
- The `getrlimit` and `setrlimit` methods of the `ulimit::set::Env` trait now
  return an error of type `Errno` instead of a `std::io::Error`.
- The `show_one` and `show_all` functions in the `ulimit::show` module now takes
  a function that returns an error of type `Errno` instead of `std::io::Error`.
- External dependency versions:
    - Rust 1.77.0 → 1.79.0
    - yash-env 0.2.0 → 0.4.0
    - yash-semantics 0.3.0 → 0.4.0 (optional)
    - yash-syntax 0.10.0 → 0.12.0
- Internal dependency versions:
    - yash-prompt 0.1.0 → 0.2.0 (optional)

## [0.3.0] - 2024-07-13

### Added

- Internal dependencies:
    - yash-prompt 0.1.0 (optional)

### Changed

- External dependency versions:
    - Rust 1.75.0 → 1.77.0
    - yash-semantics 0.2.0 → 0.3.0
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

[0.6.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.6.0
[0.5.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.5.0
[0.4.1]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.4.1
[0.4.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.4.0
[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.1.0
