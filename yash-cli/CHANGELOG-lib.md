# Changelog

All notable changes to the `yash-cli` library crate are documented in this file.

This file lists changes to the library crate, which is unlikely to be of interest
to users of the shell.
For changes to the shell binary as a whole, see [CHANGELOG-bin.md](CHANGELOG-bin.md).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0-beta.3] - Unreleased

### Added

- The `startup::init_file` module contains items for running the initialization
  files:
    - `run_rcfile`
    - `run_init_file`
    - `default_rcfile_path`
    - `resolve_rcfile_path`
    - `DefaultFilePathError`

### Changed

- The shell now executes the initialization files on startup if the shell is
  interactive.
- The `bin_main` function has been renamed to `main` and its return type is now
  `!`.
- External dependency versions:
    - Rust 1.77.0 → 1.79.0

### Fixed

- When the shell cannot open a script specified by the command-line argument,
  it now returns the exit status of 126 or 127 as required by POSIX. Previously,
  it returned the exit status of 2.

## [0.1.0-beta.2] - 2024-07-13

### Added

- Internal dependencies:
    - yash-prompt 0.1.0
- The `startup::args::Work` struct contains the `source`, `profile`, and
  `rcfile` fields which were previously in the `startup::args::Run` struct.
- The `startup::configure_environment` function implements the configuration
  of the shell environment based on the command-line arguments.

### Changed

- External dependency versions:
    - Rust 1.75.0 → 1.77.0
- Internal dependency versions:
    - yash-builtin 0.2.0 → 0.3.0
    - yash-semantics 0.2.0 → 0.3.0
    - yash-syntax 0.9.0 → 0.10.0
- The shell now shows the prompt before reading the input in the interactive mode.
  To achieve this, the `startup::prepare_input` function now applies the
  `yash_prompt::Prompter` decorator to the returned source input.
- The first argument to `startup::prepare_input` is now `env: &'a RefCell<&mut Env>`
  instead of `system: &mut SharedSystem`. This change is to allow the function to
  construct `yash_env::input::Echo` for the returned source input.
- Restructured the `startup` module:
    - `prepare_input`, `SourceInput`, and `PrepareInputError` are moved from
      `startup` to `startup::input`.
    - The `source`, `profile`, and `rcfile` fields are moved from `args::Run` to
      `args::Work`. `args::Run` now has a `work` field of type `Work`.

### Removed

- `startup::SourceInput::verbose`
    - The caller of `startup::prepare_input` is no longer responsible for setting
      the `verbose` flag of the read-eval loop. The behavior of the verbose option
      is now implemented in `yash_env::input::Echo`, which is included in
      the `startup::SourceInput::input` field.

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

[0.1.0-beta.3]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-beta.3
[0.1.0-beta.2]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-beta.1
[0.1.0-alpha.1]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-alpha.1
