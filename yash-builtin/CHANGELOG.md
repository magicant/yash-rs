# Changelog

All notable changes to `yash-builtin` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.1] - Unreleased

### Added

- The `return` built-in now supports the `--no-return` option as a synonym of
  `-n`, which returns the specified exit status without actually returning from
  the current function or script.

### Changed

- External dependency versions:
    - yash-syntax 0.15.0 → 0.15.1

### Fixed

- The `eval`, `exit`, `return`, and `shift` built-ins now correctly handle the
  `--` separator between options and operands.
- The `jobs` built-in no longer panics when reporting the same finished job more
  than once in a single invocation.

## [0.9.0] - 2025-05-11

### Added

- The new constant `cd::EXIT_STATUS_ASSIGN_ERROR` represents the exit
  status returned by the `cd` built-in when the `$PWD` or `$OLDPWD` variable is
  read-only.

### Changed

- External dependency versions:
    - yash-env 0.8.0 → 0.9.0
    - yash-semantics (optional) 0.8.0 → 0.9.0
    - yash-syntax 0.14.1 → 0.15.0
- Internal dependency versions:
    - yash-prompt (optional) 0.5.0 → 0.6.0

### Fixed

- The `cd` built-in now returns exit status 1 when the `$PWD` or `$OLDPWD`
  variable cannot be updated because it is read-only. The new constant
  `cd::EXIT_STATUS_ASSIGN_ERROR` represents this exit status. The `cd::main`
  function now returns a result that includes this exit status.
  This fix reflects the requirements in POSIX.1-2024 XBD 8.1.

## [0.8.0] - 2025-05-03

### Added

- The `getopts::report::Error` and `read::syntax::Error` enums now have the
  `InvalidVariableName` variant, which indicates that the variable name is
  invalid.

### Changed

- The `getopts::report::Error` enum now has the `InvalidVariableName` variant, which
  indicates that the variable name is invalid.
- External dependency versions:
    - yash-env 0.7.0 → 0.7.1
    - yash-semantics (optional) 0.7.0 → 0.7.1
    - yash-syntax 0.14.0 → 0.14.1

### Fixed

- The `getopts` built-in now fails when the second operand is not a valid
  variable name. The `getopts::model::Result::report` function now returns the
  `InvalidVariableName` error in this case.
- The `read` built-in now fails when a specified variable name contains an `=`
  character. The `read::syntax::parse` function returns the
  `InvalidVariableName` error in this case.
- The `export`, `readonly`, and `typeset` built-ins no longer print variables
  with a name containing an `=` character.
  The `typeset::PrintVariables::execute` function now ignores such variables.
- The `set` built-in without arguments no longer prints variables that have an
  invalid name. The `set::main` function now excludes such variables from the
  output.

## [0.7.0] - 2025-04-26

### Changed

- The `true`, `false`, and `pwd` built-ins are now substitutive, as specified in
  POSIX.1-2024.
    - In the `BUILTINS` array, these built-ins now have the `Type::Substitutive`
      type.
- The `exec` built-in implementation (`exec::main`) now accepts the `--`
  separator between options and operands, as required by POSIX.1-2024.
- As a small optimization, the `fg` built-in implementation (`fg::main`) now
  uses `yash_env::system::System::tcsetpgrp` instead of
  `yash_env::system::SystemEx::tcsetpgrp_without_block` to bring jobs to the
  foreground.
- External dependency versions:
    - Rust 1.85.0 → 1.86.0
    - yash-env 0.6.0 → 0.7.0
    - yash-semantics (optional) 0.6.0 → 0.7.0
- Internal dependency versions:
    - yash-prompt (optional) 0.4.0 → 0.5.0

## [0.6.0] - 2025-03-23

### Summary

The `cd` built-in now supports the `-e` (`--ensure-pwd`) option, which ensures
that the `$PWD` variable is set to the actual current working directory after
changing the working directory.

The `cd` built-in now errors out when a given operand is an empty string.

The command `kill -l` now shows signals in the ascending order of their numbers.

The `read` built-in now supports the `-d` (`--delimiter`) option, which allows
specifying a delimiter character to terminate the input.

The `read` built-in now returns a more specific exit status depending on the
cause of the error. It also rejects an input containing a null byte.

The `set` bulit-in now suspends itself when the `-m` option is enabled in the
background.

The `trap` built-in now implements the POSIX.1-2024 behavior of showing signal
dispositions that are not explicitly set by the user. It also supports the `-p`
(`--print`) option.

The `wait` built-in no longer treats suspended jobs as terminated jobs.

### Added

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
- `cd::syntax::Error::EmptyOperand`
    - This error variant represents a syntax error that occurs when an operand
      is an empty string.
- `cd::target::TargetError::exit_status`
    - This method returns the exit status corresponding to the error.
- `read::EXIT_STATUS_SUCCESS`, `read::EXIT_STATUS_EOF`,
  `read::EXIT_STATUS_ASSIGN_ERROR`, `read::EXIT_STATUS_READ_ERROR`,
  `read::EXIT_STATUS_SYNTAX_ERROR`
    - These constants represent exit statuses that can be returned by the `read`
      built-in.
- `read::Command::delimiter`
    - This field represents the new `-d` option of the `read` built-in.
- `read::syntax::Error::MultibyteDelimiter`
    - This error variant represents an error that occurs when a multibyte
      character is specified as a delimiter.
- `trap::Command::PrintAll::include_default`
    - This field represents the new `-p` option of the `trap` built-in used
      without operands.
- `trap::Command::Print`
    - This variant represents the new `-p` option of the `trap` built-in used
      with operands.
- `trap::syntax::OPTION_SPECS`
    - This array slice represents the option specifications of the `trap`
      built-in.
- `trap::display_all_traps`
    - This function is an extended version of `trap::display_traps` that shows
      traps including ones that have the default action.

### Changed

- The `cd::chdir::report_failure` function now returns a result with
  `EXIT_STATUS_CHDIR_ERROR`.
- The `cd::assign::new_pwd` function now returns `Result<PathBuf, Errno>` instead
  of `PathBuf`. Previously, it returned an empty `PathBuf` on failure.
- The `fg::main` and `bg::main` functions now error out if job control is not
  enabled.
- The `fg::main` function now returns with `Divert::Interrupt` when the resumed
  job is suspended.
- The `fg::main` function no longer errors out if `yash_env::Env::get_tty`
  fails.
- The `kill::print::print` function now shows signals in the ascending order of
  their numbers when given no signals.
- The `read::syntax::parse` function now accepts the `-d` (`--delimiter`) option.
- The `read::input::read` function now takes one more argument, `delimiter`, to
  specify the delimiter character.
- The `read::main` function now returns a more specific exit status depending on
  the cause of the error. It now returns `EXIT_STATUS_READ_ERROR` when finding a
  null byte in the input.
- The `set::main` function now internally calls `yash_env::Env::ensure_foreground`
  when the `-m` option is enabled.
- The `trap::syntax::interpret` function now supports the `-p` option.
- The output of the `trap` built-in now includes not only user-defined traps but
  also signal dispositions that are not explicitly set by the user.
- The `wait` built-in no longer treats suspended jobs as terminated jobs. When
  waiting for a suspended job, the built-in now waits indefinitely until the job
  is resumed and finished.
- External dependency versions:
    - Rust 1.82.0 → 1.85.0
    - yash-env 0.5.0 → 0.6.0
    - yash-semantics (optional) 0.5.0 → 0.6.0
    - yash-syntax 0.13.0 → 0.14.0
- Internal dependency versions:
    - itertools 0.13.0 → 0.14.0
    - yash-prompt (optional) 0.3.0 → 0.4.0

## [0.5.0] - 2024-12-14

### Changed

- External dependency versions:
    - yash-env 0.4.0 → 0.5.0
    - yash-semantics (optional) 0.4.0 → 0.5.0
    - yash-syntax 0.12.0 → 0.13.0
- Internal dependency versions:
    - yash-prompt (optional) 0.2.0 → 0.3.0

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
    - yash-semantics (optional) 0.3.0 → 0.4.0
    - yash-syntax 0.10.0 → 0.12.0
- Internal dependency versions:
    - yash-prompt (optional) 0.1.0 → 0.2.0

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

[0.9.1]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.9.1
[0.9.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.9.0
[0.8.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.8.0
[0.7.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.7.0
[0.6.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.6.0
[0.5.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.5.0
[0.4.1]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.4.1
[0.4.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.4.0
[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-builtin-0.1.0
