# Changelog

All notable changes to `yash-semantics` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] - Unreleased

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

[0.6.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.6.0
[0.5.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.5.0
[0.4.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.4.0
[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-semantics-0.1.0
