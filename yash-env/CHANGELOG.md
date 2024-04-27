# Changelog

All notable changes to `yash-env` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - Unreleased

### Added

- `Env::errexit_is_applicable`
- `SystemEx::fd_is_pipe`
- `SystemEx::set_blocking`

### Changed

- `system::virtual::FileSystem::get` now fails with `EACCES` when search
  permission is denied for any directory component of the path.
- The type parameter constraint for `subshell::Subshell` is now
  `F: for<'a> FnOnce(&'a mut Env, Option<JobControl>) -> Pin<Box<dyn Future<Output = ()> + 'a>> + 'static`.
  The `Output` type of the returned future has been changed from
  `semantics::Result` to `()`.

### Removed

- `semantics::apply_errexit`

### Fixed

- `RealSystem::open_tmpfile` no longer returns a file descriptor with the
  `O_CLOEXEC` flag set.
- `<RealSystem as System>::is_executable_file` now uses `eaccess` instead of
  `access` to check execute permission.

## [0.1.0] - 2024-04-13

### Added

- Initial implementation of the `yash-env` crate

[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.1.0
