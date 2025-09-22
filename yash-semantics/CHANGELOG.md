# Changelog

All notable changes to `yash-semantics` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.0] - Unreleased

### Added

- Support for the `pipefail` shell option in pipeline execution
    - When this option is enabled, the exit status of a pipeline reflects the
      failure of any command in the pipeline, not just the last command.
    - This is implemented in
      `impl command::Command for yash_syntax::syntax::Pipeline`.
- The `redir::ErrorCause` enum now has the `UnsupportedPipeRedirection` and
  `UnsupportedHereString` variants to indicate the use of unsupported pipe
  redirections (`>>|`) and here-string redirections (`<<<`), respectively.
    - These variants are returned by `redir::RedirGuard::perform_redir`
      when such redirections are encountered.

### Changed

- The special parameter `!` is now considered unset if no asynchronous command
  has been executed, that is, if `JobList::last_async_pid()` is zero.
    - This change is observed in the results of the `expand` method of
      `impl expansion::initial::Expand for yash_syntax::syntax::TextUnit`.
- `redir::ErrorCause` is now marked as `non_exhaustive`.
- External dependency versions:
    - yash-env 0.8.0 → 0.8.1
    - yash-syntax 0.15.1 → 0.15.2

## [0.8.1] - 2025-09-20

### Added

- The `command_search::classify` function has been added to classify a command
  name without searching for an external utility.
    - This function is used in the execution of a simple command
      (`impl command::Command for yash_syntax::syntax::SimpleCommand`) to classify
      the command name before performing redirections and variable assignments
      without redundant searching for an external utility.
- The `command_search::ClassifyEnv` trait has been added to provide the
  environment required by the `command_search::classify` function.
    - This trait is a subset of the `command_search::SearchEnv` trait.
- Internal dependencies:
    - either 1.9.0

### Changed

- External dependency versions:
    - yash-syntax 0.15.0 → 0.15.1

### Removed

- Internal dependencies:
    - assert_matches 1.5.0

### Fixed

- The execution of a simple command
  (`impl command::Command for yash_syntax::syntax::SimpleCommand`)
  now searches for the external utility in the `PATH` after performing variable
  assignments, as specified in POSIX.1-2024. Previously, it would search for the
  utility before performing the redirections and assignments, which could lead
  to incorrect behavior if the assignments modified the `PATH` variable.

## [0.8.0] - 2025-05-11

### Changed

- When a tilde expansion produces a directory name that ends with a slash and
  the expansion is followed by a slash, the trailing slash in the directory name
  is now removed to maintain the correct number of slashes.
    - This is done by removing the trailing slash from the directory name in
      `impl expansion::initial::Expand for WordUnit`. This is done only if the
      `followed_by_slash` flag is set to `true` in the tilde expansion
      (`WordUnit::Tilde`).
- External dependency versions:
    - yash-env 0.7.1 → 0.8.0
    - yash-syntax 0.14.1 → 0.15.0

## [0.7.1] - 2025-05-03

### Changed

- When a field is made up of a single tilde expansion that expands to an empty
  string, the expanded field is no longer removed from the command line.
    - This is done by producing a dummy quote in the tilde expansion
      (`impl expansion::initial::Expand for WordUnit`).
- External dependency versions:
    - yash-env 0.7.0 → 0.7.1
    - yash-syntax 0.14.0 → 0.14.1

## [0.7.0] - 2025-04-26

### Changed

- In pathname expansion (`expansion::glob`), pathname component patterns no
  longer expand to the filename `.` or `..`.
- When a value is assigned to a variable in an expansion of the form
  `${name=word}` or `${name:=word}`, the resulting expansion is now the value of
  the variable after the assignment, rather than the expansion of `word`.
    - In `impl expansion::initial::Expand for TextUnit`, the `expand` method now
      returns the value of the variable after the assignment instead of the
      expansion of `word`. All characters in the result now have the `is_quoted`
      flag set to `false`, so they will be subject to field splitting and
      pathname expansion even if they were quoted in the original `word`.
- When an asynchronous command is executed
  (`impl command::Command for yash_syntax::syntax::Item`) in an interactive
  shell (`yash_env::Env::is_interactive`), the job number and the process ID are
  now printed to the standard error, as required by POSIX.1-2024.
- When an asynchronous command is executed
  (`impl command::Command for yash_syntax::syntax::Item`), the job representing
  the command now has the `yash_env::job::Job::state_changed` flag set to
  `false` to avoid re-reporting the job state change before the next prompt.
- The execution of a pipeline
  (`impl command::Command for yash_syntax::syntax::Pipeline`)
  no longer calls `yash_env::system::SystemEx::tcsetpgrp_without_block` before
  starting the pipeline even if job control is enabled. Previously, it would
  call this function to make sure that the shell is in the foreground process
  group before creating a new process group for the pipeline. Now, this
  operation is not performed as the shell is expected to already be in the
  foreground process group when executing a pipeline.
- The shell now returns an exit status of 128 on an I/O error in the parser
  except when the error location is in a script read by the `.` built-in.
    - In `impl Handle for yash_syntax::parser::Error`, the `handle` method now
      returns `yash_env::semantics::ExitStatus::READ_ERROR` instead of
      `yash_env::semantics::ExitStatus::ERROR` if the error cause is an I/O
      error and the error location is not
      `yash_syntax::source::Source::DotScript`.
- External dependency versions:
    - Rust 1.85.0 → 1.86.0
    - yash-env 0.6.0 → 0.7.0

## [0.6.0] - 2025-03-23

### Added

- Added the `expand_word_multiple` and `expand_word_with_mode` functions to the
  `expansion` module.
- Added the `job` module, which contains the `add_job_if_suspended` utility
  function.

### Changed

- The execution of a simple command
  (`impl command::Command for yash_syntax::syntax::SimpleCommand`)
  now honors the `ExpansionMode` specified for the words in the command.
- The `command::simple_command::start_external_utility_in_subshell_and_wait`
  function now returns `Result<ExitStatus>` instead of `ExitStatus`.
- When a foreground job is suspended in an interactive shell, the shell now
  discards any remaining commands in the current command line and prompts for
  the next command line.
- External dependency versions:
    - Rust 1.82.0 → 1.85.0
    - yash-env 0.5.0 → 0.6.0
    - yash-syntax 0.13.0 → 0.14.0
- Internal dependency versions:
    - itertools 0.13.0 → 0.14.0

## [0.5.0] - 2024-12-14

### Changed

- `<yash_syntax::syntax::CompoundCommand as command::Command>::execute` now
  honors the `CaseContinuation` specified for the executed case item.
- `<yash_syntax::syntax::WordUnit as expansion::Expand>::expand` now supports
  expanding dollar-single-quotes.
- External dependency versions:
    - Rust 1.79.0 → 1.82.0
    - yash-env 0.4.0 → 0.5.0
    - yash-syntax 0.12.0 → 0.13.0
- Internal dependency versions
    - thiserror 1.0.47 → 2.0.4

### Fixed

- The `interactive_read_eval_loop` function now flushes the lexer on recovery
  from a syntax error, so that the next line is read in a fresh state.
  Previously, the lexer would continue from the next token after the error,
  confusingly parsing the rest of the line before reading the next line.

## [0.4.0] - 2024-09-29

### Added

- This crate now builds on non-Unix platforms.
- `interactive_read_eval_loop`
    - This function is an extension of the `read_eval_loop` function for
      interactive shells.
- Error types in the `expansion` module (some of which are reexported in the
  `assign` module) have been extended for more informative error messages:
    - The `ErrorCause::footer` method has been added.
    - The `Error` struct now has non-default implementation of the
      `MessageBase::footers` method.
    - The `AssignReadOnlyError` struct now has a `vacancy: Option<Vacancy>`
      field.
    - The `initial::VacantError` struct now has a `param: Param` field.
    - The `initial::NonassignableErrorCause` enum is a successor to the previous
      `NonassignableError` enum. The new `NotVariable` variant has a `param:
      Param` field.

### Changed

- Error types in the `expansion` module (some of which are reexported in the
  `assign` module) have been extended for more informative error messages:
    - The `ErrorCause::UnsetParameter` variant now has a `param: Param` field.
    - The `message` and `label` methods of `ErrorCause` return more informative
      messages for the `UnsetParameter` and `VacantExpansion` variants.
    - The `expansion::initial::NonassignableError` enum has been replaced with a
      struct of the same name so that it can have a `Vacancy` field.
    - The `MessageBase::additional_annotations` method implementation for the
      `Error` struct has been extended to produce more annotations for errors
      with `Vacancy` information.
- The pipeline execution now ignores the `noexec` option in interactive shells.
    - Previously, the `<yash_syntax::syntax::Pipeline as
      command::Command>::execute` method skipped the execution of the pipeline
      if the `Exec` shell option was off. Now, it skips the execution only if
      the `Exec` and `Interactive` shell options are both off.
- External dependency versions:
    - Rust 1.77.0 → 1.79.0
    - yash-env 0.2.0 → 0.4.0
    - yash-syntax 0.10.0 → 0.12.0

## [0.3.0] - 2024-07-13

### Added

- `read_eval_loop`
    - This function replaces the `ReadEvalLoop` struct and its methods.
      It supports the `yash_env::input::Echo` decorator by taking a
      `&RefCell<&mut Env>`.
- `ReadEvalLoop` now has the `must_use` attribute.

### Changed

- External dependency versions:
    - Rust 1.75.0 → 1.77.0
    - yash-syntax 0.9.0 → 0.10.0

### Deprecated

- `ReadEvalLoop::set_verbose` in favor of `yash_env::input::Echo`

### Removed

- Internal dependencies:
    - futures-util 0.3.28

### Fixed

- Small performance improvements

## [0.2.0] - 2024-06-09

### Added

- Support for the `ErrExit` shell option in multi-command pipelines

### Changed

- External dependency versions
    - yash-env 0.1.0 → 0.2.0
    - yash-syntax 0.8.0 → 0.9.0
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
- `<yash_syntax::syntax::FunctionDefinition as command::Command>::execute` now
  prints its error message in prettier format.
- `command::simple_command::replace_current_process` now uses `System::shell_path`
  to find the shell executable when it needs to fall back to the shell.
- `trap::run_trap_if_caught` now takes a `yash_env::signal::Number` argument
  instead of a `yash_env::trap::Signal`.

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

[0.9.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.9.0
[0.8.1]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.8.1
[0.8.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.8.0
[0.7.1]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.7.1
[0.7.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.7.0
[0.6.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.6.0
[0.5.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.5.0
[0.4.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.4.0
[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.1.0
