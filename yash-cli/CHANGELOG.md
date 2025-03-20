# Changelog

All notable changes to the shell are documented in this file.

This file lists changes to the shell executable as a whole. Changes to the
implementing library crate are not documented since it is not intended to be
used by other programs.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - Unreleased

### Added

- The shell now supports declaration utilities as defined in POSIX.
- The `cd` built-in now supports the `-e` option as defined in POSIX.
- The `read` built-in now supports the `-d` (`--delimiter`) option, which allows
  specifying a delimiter character to terminate the input.
- The `trap` built-in now implements the POSIX.1-2024 behavior of showing
  signal dispositions that are not explicitly set by the user. It also supports
  the `-p` (`--print`) option.
- The `-p` option for the `command` built-in now works on Linux.

### Changed

- When a foreground job is suspended in an interactive shell, the shell now
  discards any remaining commands in the current command line and prompts for
  the next command line. This behavior basically conforms to POSIX.1-2024, but
  differs in that the shell does not resume with the remaining commands
  following the next asynchronous and-or list.
- The `cd` built-in now errors out when a given operand is an empty string.
- The `cd` built-in now returns different exit statuses for different errors.
- The command `kill -l` now shows signals in the ascending order of their
  numbers.
- The `read` built-in now returns a more specific exit status depending on the
  cause of the error. It also rejects an input containing a null byte.
- The output of the `trap` built-in now includes not only user-defined traps but
  also signal dispositions that are not explicitly set by the user.
- The `wait` built-in no longer treats suspended jobs as terminated jobs. When
  waiting for a suspended job, the built-in now waits indefinitely until the job
  is resumed and finished.

## [0.2.0] - 2024-12-14

### Added

- A case branch now can be terminated with `;&` or `;|` instead of `;;` to fall
  through to the next branch or to resume pattern matching from the next branch,
  respectively. `;&` is a POSIX.1-2024 feature, and `;|` is an extension. For
  compatibility with other shells, `;;&` is also accepted as an alias for `;|`.
- Dollar-single-quotes are now supported as a form of quoting where backslash
  escapes are recognized.
    - Currently, octal and hexadecimal escapes that expand to a value greater
      than 127 are translated to a UTF-8 sequence for the corresponding Unicode
      scalar value. This behavior does not conform to POSIX.1-2024 and is
      subject to change.
    - As an extension to POSIX.1-2024, the shell also recognizes the `\u` and
      `\U` escapes for Unicode scalar values, and the `\E` escape as a synonym
      for `\e`.

### Changed

- The shell's syntax now allows `esac` as the first pattern of a case branch
  as in `case esac in (esac|case) echo ok; esac`. Previously, it was a syntax
  error, but POSIX.1-2024 allows it.
- The `bg` built-in now updates the `!` special parameter to the process ID of
  the background job, as required by POSIX.1-2024.
- The `exec` built-in no longer exits the shell when the specified command is
  not found in an interactive shell, as required by POSIX.1-2024.

### Fixed

- The interactive shell now discards the entire line when a syntax error occurs
  in the middle of a command line. Previously, it would continue parsing the
  rest of the line, which could lead to confusing behavior.

## [0.1.0] - 2024-09-29

### Added

- The shell now runs the initialization file specified by the `ENV` environment
  variable if it is set and the shell is interactive.

### Changed

- The shell now rejects an invalid parameter as a syntax error. Specifically,
  if a parameter starts with a digit but is not a valid number, the shell now
  reports a syntax error instead of treating it as a variable. For example,
  `${1abc}` and `${0_1}` are now syntax errors.
- Improved error messages for some parameter expansion errors.
- Interactive shells now report updates to job status before showing the prompt.
- Interactive shells no longer exit on shell errors such as syntax errors.
- Interactive shells now ignore the `noexec` option.
- Interactive shells now support the `ignoreeof` option.
- Interactive shells now allow modifying the trap for signals that were ignored
  on the shell startup.

### Fixed

- When the shell cannot open a script specified by the command-line argument,
  it now returns the exit status of 126 or 127 as required by POSIX. Previously,
  it returned the exit status of 2.

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

[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0
[0.1.0-beta.2]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-beta.1
[0.1.0-alpha.1]: https://github.com/magicant/yash-rs/releases/tag/yash-cli-0.1.0-alpha.1
