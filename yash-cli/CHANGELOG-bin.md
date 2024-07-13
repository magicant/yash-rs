# Changelog

All notable changes to the shell are documented in this file.

This file lists changes to the shell binary as a whole. Changes to the
implementing library crate are in [CHANGELOG-lib.md](CHANGELOG-lib.md).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0-beta.2] - 2024-07-13

### Changed

- The shell now shows the prompt before reading the input in the interactive mode.

### Fixed

- The break and continue built-ins no longer allow exiting a trap.
- The read built-in now shows a prompt when reading a continued line.
- The source built-in now echoes the input when the verbose shell option is set.
- The set built-in no longer sets the `SIGTTIN`, `SIGTTOU`, and `SIGTSTP` signals
  to be ignored when invoked with the `-m` option in a subshell of an
  interactive shell.

## [0.1.0-beta.1] - 2024-06-09

### Changed

- The shell now enables blocking reads on the standard input if it is a terminal
  or a pipe as [required by POSIX](https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/utilities/sh.html#tag_20_117_06).

## [0.1.0-alpha.1] - 2024-04-13

### Added

- Initial release of the shell

[0.1.0-beta.2]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-beta.1
[0.1.0-alpha.1]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-alpha.1
