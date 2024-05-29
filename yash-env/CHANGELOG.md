# Changelog

All notable changes to `yash-env` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - Unreleased

### Added

- `Env::errexit_is_applicable`
- `System::shell_path`
- `System::{validate_signal, signal_number_from_name}`
- `SystemEx::signal_name_from_number`
- `SystemEx::fd_is_pipe`
- `SystemEx::set_blocking`
- `job::ProcessResult`
- `job::ProcessResult::{exited, is_stopped}`
- `impl From<job::ProcessResult> for ExitStatus`
- `job::ProcessState::Halted`
- `job::ProcessState::{stopped, exited, is_stopped}`
- `impl Hash for job::fmt::Marker`
- `job::fmt::State`
- `impl From<trap::Condition> for stack::Frame`
- `signal::{Number, RawNumber, Name, NameIter, UnknownNameError}`

### Changed

- `stack::Frame` is now `non_exhaustive`.
- `job::fmt::Report` has been totally rewritten.
- `system::virtual::FileSystem::get` now fails with `EACCES` when search
  permission is denied for any directory component of the path.
- The following methods of `system::virtual::Process` now operate on
  `signal::Number` instead of `trap::Signal`:
    - `signal_handling`
    - `set_signal_handling`
- The type parameter constraint for `subshell::Subshell` is now
  `F: for<'a> FnOnce(&'a mut Env, Option<JobControl>) -> Pin<Box<dyn Future<Output = ()> + 'a>> + 'static`.
  The `Output` type of the returned future has been changed from
  `semantics::Result` to `()`.

### Removed

- `job::ProcessState::{Exited, Signaled, Stopped}` in favor of `job::ProcessResult`
- `job::ProcessState::to_wait_status`
- `semantics::apply_errexit`

### Fixed

- `RealSystem::open_tmpfile` no longer returns a file descriptor with the
  `O_CLOEXEC` flag set.
- `<RealSystem as System>::is_executable_file` now uses `faccessat` with
  `AT_EACCESS` instead of `access` to check execute permission (except on
  Redox).

## [0.1.0] - 2024-04-13

### Added

- Initial implementation of the `yash-env` crate

[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.1.0
