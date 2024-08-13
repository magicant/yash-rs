# Changelog

All notable changes to `yash-env` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - Unreleased

### Added

- The `OfdAccess`, `OpenFlag`, `FdFlag`, `Mode`, `RawMode`, `Uid`, `RawUid`,
  `Gid`, `RawGid`, `FileType`, `Stat`, and `SigmaskOp` types in the `system`
  module
- The `System` trait now has the `ofd_access`, `get_and_set_nonblocking`,
  `getuid`, `geteuid`, `getgid`, and `getegid` methods.
- `Mode` has been moved from `system::virtual` to `system` and now has constants
  with more human-friendly names, e.g., `USER_READ` and `GROUP_WRITE`.
- The `system::virtual::Inode` struct now has the `stat` method.
- The `system::virtual::FileBody` struct now has the `type` and `size` methods.
- The `system::virtual::Process` struct now has the getters/setters for the
  real/effective user/group IDs: `uid`, `set_uid`, `euid`, `set_euid`, `gid`,
  `set_gid`, `egid`, and `set_egid`.
- `system::virtual::VirtualSystem::open` now applies the process's umask to the
  mode argument.
- The `job::RawPid` type has been added to represent the contents of `job::Pid`.
- The `stack::Frame` enum now has the `InitFile` variant.
- External dependencies:
    - enumset 1.1.2 (previously an internal dependency)
- Internal dependencies:
    - bitflags 2.6.0

### Changed

- `system::FdFlag` is no longer a re-export of `nix::fcntl::FdFlag`.
- `system::Mode` is no longer a re-export of `nix::sys::stat::Mode`.
- The `fstat` and `fstatat` methods of `system::System` now return a `Stat`
  instead of a `nix::sys::stat::FileStat`.
- The `system::System::fstatat` method now takes a `follow_symlinks: bool`
  parameter instead of an `AtFlags` parameter.
- The `system::System::open` method has been redefined to take `OfdAccess` and
  `OpenFlag` parameters instead of `nix::fcntl::OFlag`.
- The `system::System::umask` method now takes and returns a value of the new
  `system::Mode` type.
- The `system::System::sigmask` method now takes a `SigmaskOp` parameter instead
  of a `nix::sys::signal::SigmaskHow` parameter.
- The `system::System::select` method now takes `Vec<io::Fd>` parameters instead
  of `fd_set::FdSet` parameters. It also takes a `Duration` instead of a
  `nix::sys::time::TimeSpec` for the optional timeout parameter.
- The `dup`, `fcntl_getfl`, and `fcntl_setfl` methods of `system::System` now
  operate on an `EnumSet<FdFlag>` parameter instead of an `nix::fcntl::FdFlag`
  parameter.
- The `getrlimit` and `setrlimit` methods of `system::System` now returns an
  error of type `system::Errno` instead of `std::io::Error`.
- In the `system::resource` module:
    - The `rlim_t` type has been renamed to `Limit`.
    - The `RLIM_INFINITY` constant has been renamed to `INFINITY`.
- The `flags: enumset::EnumSet<FdFlag>` field of
  `yash_env::system::virtual::FdBody` has replaced
  the `flag: nix::fcntl::FdFlag` field.
- The `system::virtual::INode` struct has been renamed to `Inode`.
- The `system::virtual::OpenFileDescription::i_node` method has been renamed to
  `inode` and now returns a reference to `Rc<RefCell<Inode>>` rather than a
  clone of it.
- The `system::virtual::OpenFileDescription::seek` method now takes a
  `std::io::SeekFrom` parameter instead of an offset and whence pair.
- The `system::virtual::VirtualSystem::select` method now treats as ready file
  descriptors that are contained in `readers` but not readable, or in `writers`
  but not writable. Previously, the method returned an `EBADF` error in these
  cases.
- External dependency versions:
    - Rust 1.77.0 → 1.79.0

### Deprecated

- `system::virtual::Mode` in favor of `system::Mode`

### Removed

- The `system` module no longer reexports `nix::fcntl::AtFlags`,
  `nix::fcntl::OFlag`, `nix::sys::stat::FileStat`, `nix::sys::stat::SFlag`,
  `nix::sys::signal::SigmaskHow`, and `nix::sys::time::TimeSpec`.
- The `fcntl_getfl` and `fcntl_setfl` methods from the `System` trait
- The `system::Errno` struct's `last` and `clear` methods are no longer public.
- The `system::resource::Resource::as_raw_type` method is no longer public.
- All the fields of the `system::virtual::OpenFileDescription` struct are now
  private.
- The `system::fd_set` module
- `impl TryFrom<semantics::ExitStatus> for nix::sys::signal::Signal`
- `impl From<system::resource::LimitPair> for nix::libc::rlimit`
- `impl From<nix::libc::rlimit> for system::resource::LimitPair`

## [0.2.1] - 2024-07-12

### Added

- `Env::is_interactive`
- `impl yash_system::alias::Glossary for Env`
- `input::Echo`
    - This is a decorator of `Input` that implements the behavior of the verbose shell option.
- `input::FdReader` is now marked `#[must_use]`.
- `variable::VariableSet::get_scalar`
    - This is a convenience method that returns a scalar variable as a `Cow<str>`.
- Variable name constants in the `variable` module:
  `CDPATH`, `ENV`, `HOME`, `IFS`, `LINENO`, `OLDPWD`, `OPTARG`, `OPTIND`,
  `PATH`, `PPID`, `PS1`, `PS2`, `PS4`, `PWD`
- Variable initial value constants in the `variable` module:
  `IFS_INITIAL_VALUE`, `OPTIND_INITIAL_VALUE`, `PS1_INITIAL_VALUE_NON_ROOT`,
  `PS1_INITIAL_VALUE_ROOT`, `PS2_INITIAL_VALUE`, `PS4_INITIAL_VALUE`
- `impl System for &SharedSystem` and `impl trap::SignalSystem for &SharedSystem`
    - This allows `SharedSystem` to be used as a system behind a non-mutable reference.

### Changed

- External dependency versions:
    - Rust 1.70.0 → 1.77.0
    - yash-syntax 0.9.0 → 0.10.0
- Internal dependency versions:
    - annotate-snippets 0.10.0 → 0.11.4
- All inherent methods of `SharedSystem` now take `&self` instead of `&mut self`:
    - `SharedSystem::read_async`
    - `SharedSystem::write_all`
    - `SharedSystem::print_error`

### Deprecated

- `input::FdReader::set_echo` in favor of `input::Echo`

### Fixed

- `stack::Stack::loop_count` no longer counts loops below a `Frame::Trap(_)` frame.
- Possible undefined behavior in `RealSystem::times`

## [0.2.0] - 2024-06-09

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
- `trap::SignalSystem::signal_name_from_number`
- `trap::SignalSystem::signal_number_from_name`
- `ExitStatus::to_signal_number`

### Changed

- External dependency versions
    - yash-syntax 0.8.0 → 0.9.0
- `stack::Frame` is now `non_exhaustive`.
- `job::fmt::Report` has been totally rewritten.
- `system::virtual::FileSystem::get` now fails with `EACCES` when search
  permission is denied for any directory component of the path.
- `job::ProcessResult::{Stopped, Signaled}` now have associated values of type
  `signal::Number` instead of `trap::Signal`.
- The following methods now operate on `signal::Number` instead of `trap::Signal`:
    - `Env::wait_for_signal`
    - `Env::wait_for_signals`
    - `job::ProcessState::stopped`
    - `system::System::caught_signals`
    - `system::System::kill`
    - `system::System::sigaction`
    - `system::System::sigmask`
    - `system::System::select`
    - `system::virtual::Process::blocked_signals`
    - `system::virtual::Process::pending_signals`
    - `system::virtual::Process::block_signals`
    - `system::virtual::Process::signal_handling`
    - `system::virtual::Process::set_signal_handling`
    - `trap::TrapSet::catch_signal`
    - `trap::TrapSet::take_caught_signal`
    - `trap::TrapSet::take_signal_if_caught`
- `system::System::sigmask` now takes three parameters:
    1. `&mut self,`
    2. `op: Option<(SigmaskHow, &[signal::Number])>,`
    3. `old_mask: Option<&mut Vec<signal::Number>>,`
- `system::virtual::SignalEffect::of` now takes a `signal::Number` parameter
  instead of a `trap::Signal`. This function is now `const`.
- The type parameter constraint for `subshell::Subshell` is now
  `F: for<'a> FnOnce(&'a mut Env, Option<JobControl>) -> Pin<Box<dyn Future<Output = ()> + 'a>> + 'static`.
  The `Output` type of the returned future has been changed from
  `semantics::Result` to `()`.

### Removed

- `job::ProcessState::{Exited, Signaled, Stopped}` in favor of `job::ProcessResult`
- `job::ProcessState::{from_wait_status, to_wait_status}`
- `impl std::fmt::Display for job::ProcessResult`
- `impl std::fmt::Display for job::ProcessState`
- `impl From<Signal> for ExitStatus`
- `semantics::apply_errexit`
- `trap::Signal`
- `trap::ParseConditionError`

### Fixed

- `RealSystem::open_tmpfile` no longer returns a file descriptor with the
  `O_CLOEXEC` flag set.
- `<RealSystem as System>::is_executable_file` now uses `faccessat` with
  `AT_EACCESS` instead of `access` to check execute permission (except on
  Redox).

## [0.1.0] - 2024-04-13

### Added

- Initial implementation of the `yash-env` crate

[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.3.0
[0.2.1]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.2.1
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.1.0
