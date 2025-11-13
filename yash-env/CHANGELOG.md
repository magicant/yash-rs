# Changelog

All notable changes to `yash-env` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Terminology: A _public dependency_ is one that’s exposed through this crate’s
public API (e.g., re-exported types).
A _private dependency_ is used internally and not visible to downstream users.

## [0.10.0] - Unreleased

### Added

- `parser::IsName`: Wrapper for a function that checks if a string is a valid
  variable name. This allows modules to check variable names without directly
  depending on the `yash-syntax` crate.

### Removed

- `parser::is_name`: This re-export of `yash_syntax::parser::lex::is_name` has
  been removed in favor of the new `IsName` dependency injection pattern.

## [0.9.2] - 2025-11-07

### Added

- `source` module
    - This module re-exports `Code`, `Location`, `Source`, and `pretty` from
      `yash_syntax::source`.
- `parser` module
    - `Config`: Configuration for the parser.
    - `IsKeyword`: Function that checks if a string is a reserved word.
    - `is_name`: Function that checks if a string is a valid variable name.
- `alias` module
    - This module re-exports `Alias`, `AliasSet`, `Glossary`, and `HashEntry`
      from `yash_syntax::alias`.
- `job::add_job_if_suspended`
    - This function adds a job to the job list if the given process is
      suspended and job control is enabled.
- `prompt` module
    - This module currently contains the `GetPrompt` struct, which wraps
      a prompt-generating function.
- `semantics::RunReadEvalLoop`
    - This struct wraps a function that runs the read-eval loop.
- `trap::RunSignalTrapIfCaught`
    - This struct wraps a function that runs a signal trap if the signal has been
      caught.
- `semantics::command` module
    - `ReplaceCurrentProcessError`: Error returned when `replace_current_process`
      fails.
    - `RunFunction`: Struct that wraps a function for invoking shell functions.
    - `StartSubshellError`: Error returned when starting a subshell fails.
    - `replace_current_process`: Function that replaces the current process
      with an external utility.
    - `run_external_utility_in_subshell`: Function that runs an external utility
      in a subshell.
    - `search`: module for command search functionality.
- `semantics::expansion` module
    - The content of this module has been moved from `yash_semantics::expansion`
      to here for better modularity. Currently, it contains the following
      submodules:
        - `semantics::expansion::attr` module
            - `AttrChar`: Character with attributes describing its origin
            - `AttrField`: String of attributed characters
            - `Origin`: Category of syntactic elements from which expansion originates
        - `semantics::expansion::attr_strip` module
            - `Strip`: Trait for performing attribute stripping
            - `Iter`: Iterator wrapper that performs attribute stripping on items
        - `semantics::expansion::quote_removal` module
            - `remove_quotes` and `skip_quotes`: Functions for removing quotes from
              attributed fields
        - `semantics::expansion::split` module
            - `Class`: Type of characters that affect field splitting
            - `Ifs`: Collection of input field separator characters
            - `Ranges`: Iterator that yields index ranges of separated fields
            - `split_into` and `split`: Functions for performing field splitting
- Private dependencies:
    - derive_more 2.0.1

## [0.9.1] - 2025-10-18

### Changed

- The `times` method of `RealSystem` now uses `getrusage` instead of `times`
  to get CPU times. This provides better resolution on many systems.

## [0.9.0] - 2025-10-13

### Added

- `io::report_to_string`
    - This function converts a `yash_syntax::source::pretty::Report` to a string.
- `io::print_report`
    - This function prints a `yash_syntax::source::pretty::Report` to the standard
      error output of the given environment.
- `expansion::Error::to_report`
    - This method converts an expansion error to a `yash_syntax::source::pretty::Report`.

### Changed

- Public dependency versions:
    - yash-syntax 0.15.0 → 0.16.0
- Private dependency versions:
    - annotate-snippets 0.11.4 → 0.12.4

### Deprecated

- `io::message_to_string` in favor of `io::report_to_string`
- `io::print_message` in favor of `io::print_report`

## [0.8.1] - 2025-09-23

### Added

- The `pipefail` shell option (`option::Option::PipeFail`)

### Changed

- Public dependency versions:
    - yash-syntax 0.15.0 → 0.15.2
- Private dependency versions:
    - libc 0.2.169 → 0.2.171

## [0.8.0] - 2025-05-11

### Changed

- Public dependency versions:
    - yash-syntax 0.14.1 → 0.15.0

## [0.7.1] - 2025-05-03

### Changed

- Public dependency versions:
    - yash-syntax 0.14.0 → 0.14.1

### Fixed

- `system::SharedSystem::wait_for_signals` now returns signals that have already
  been caught before the call to `wait_for_signals`, if any. Previously, signals
  that have been caught were ignored.
- `variable::VariableSet::env_c_strings` now excludes variables with a name that
  contains a `=` character because the variable name would never be interpreted
  correctly by the program that would receive the exported variable.

## [0.7.0] - 2025-04-26

### Added

- The `System` trait now has the `getsid` method.
    - This method returns the session ID of the process with the given PID.
- The `System` trait now has the `exit` method.
    - This method terminates the process with the given exit status.
- The `System` trait now has the `raise` method.
    - This method sends a signal to the calling process.
- The `SystemEx` trait now has the `exit_or_raise` method.
    - This method terminates the current process with the given exit status,
      possibly sending a signal to kill the process.
- The `semantics::ExitStatus` struct now has the `READ_ERROR` constant.
    - This constant represents an exit status indicating an unrecoverable
      read error. Its value is 128.
- The `semantics::ExitStatus` struct now has the `to_signal` method.
    - This method converts the exit status to a signal name and number if
      applicable.
- The `system::FlexFuture` enum has been added.
    - This enum is returned by some `System` methods to allow optimizing
      the return value of the method.
- Private dependencies:
    - dyn-clone 1.0.19

### Changed

- The definition of `system::ChildProcessTask` is updated so that the `Output`
  type of the returned `Future` is now `std::convert::Infallible` instead of
  `()`.
- The return value of `System::execve` is now wrapped in a future.
- The methods of `System` that return a future now return `FlexFuture` instead
  of `Pin<Box<dyn Future>>`:
    - `System::exit`
    - `System::kill`
    - `System::raise`
- Public dependency versions:
    - Rust 1.85.0 → 1.86.0

### Fixed

- The `Env::ensure_foreground` method now correctly determines if the shell
  is in the same process group as the session leader. Previously, it
  incorrectly assumed that the shell was in the session leader's process
  group if the shell was in the foreground.

### Removed

- The `SystemEx` trait no longer has the `signal_number_from_exit_status` method.
    - This method has been moved in favor of `semantics::ExitStatus::to_signal`.
- The `semantics::ExitStatus` struct no longer has the `to_signal_number` method.
    - This method has been moved in favor of `semantics::ExitStatus::to_signal`.

## [0.6.0] - 2025-03-23

### Added

- The `Env` struct now implements `yash_syntax::decl_util::Glossary`.
- The `Env` struct now contains the `any` field of type `DataSet`.
    - The `DataSet` struct is defined in the newly added `any` module.
      It can be used to store arbitrary data.
- The `Env` struct now has the `wait_for_subshell_to_halt` method.
    - This method waits for the subshell to terminate or stop.
- The `Env` struct now has the `ensure_foreground` method.
    - This method ensures that the shell is in the foreground.
- The `System` trait now has the `get_sigaction` method.
    - This method returns the current signal handling configuration for a signal.
      This method does not modify anything, so it can be used with an immutable
      reference to the system.
- The `builtin::Builtin` struct now has the `is_declaration_utility` field.
- The `builtin::Builtin` struct now can be constructed with the associated
  function `new`.
- The `trap::Condition` enum now has the `iter` associated function.
    - Given a `SignalSystem` implementation, this function returns an iterator
      that yields all the conditions available in the system.
- The `trap::SignalSystem` trait now has the `get_disposition` method.
- The `Origin` enum has been added to the `trap` module.
    - This enum represents the origin of a configured trap.
- The `trap::TrapSet` struct now implements `Default`.
- The `trap::TrapSet` struct now has the `peek_state` method.
    - This method can be used to get the current state of a trap by accessing
      the underlying system if necessary. It reports the trap state that should
      be used in the output of the `trap` built-in.
- The `system::errno::Errno` struct now can be converted to and from the `Errno`
  type from the `errno` crate.
- The `SystemEx` trait now has the `signal_number_from_exit_status` method.
    - This method converts an exit status to a signal number if applicable.
- Private dependencies:
    - errno 0.3.10
    - libc 0.2.169

### Changed

- `Env::get_tty` now uses `system::OpenFlag::NoCtty` when opening the terminal
  device file.
- `System::getpwnam_dir` now takes a `&CStr` parameter instead of a `&str`.
- The `builtin::Builtin` struct is now `non_exhaustive`.
- The `subshell::Subshell::start` method no longer fails if `Env::get_tty` fails.
- The `origin` field of the `trap::TrapState` struct is now `trap::Origin`.
- The `TrapSet::get_state` method now returns a `TrapState` reference even if
  the current action was not set by the user.
- The `trap::Iter` iterator now yields
  `(&'a Condition, &'a TrapState, Option<&'a TrapState>)` instead of
  `(&'a Condition, Option<&'a TrapState>, Option<&'a TrapState>)`.
  It now yields the current state even if the current action was not set by the
  user.
- Public dependency versions:
    - Rust 1.82.0 → 1.85.0
    - yash-syntax 0.13.0 → 0.14.0
- Private dependency versions:
    - itertools 0.13.0 → 0.14.0
    - strum 0.26.2 → 0.27.0

### Removed

- The implementation of `From` for converting `errno::Errno` to and from
  `nix::errno::Errno`
- The `getopts_state` field from the `Env` struct
- The `builtin::getopts` module and its contents
  (the `GetoptsState` struct and the `Origin` enum)
- Private dependencies:
    - futures-util 0.3.31
    - nix 0.29.0

## [0.5.0] - 2024-12-14

### Changed

- Public dependency versions:
    - Rust 1.79.0 → 1.82.0
    - yash-syntax 0.12.0 → 0.13.0
- Private dependency versions:
    - futures-util 0.3.28 → 0.3.31
    - thiserror 1.0.47 → 2.0.4

## [0.4.0] - 2024-09-29

### Added

- `job::fmt::Accumulator`
    - This is a utility for formatting status reports of multiple jobs.
- `input::Reporter`
    - This `Input` decorator reports job status changes to the user.
- `input::IgnoreEof`
    - This `Input` decorator implements the behavior of the `ignoreeof` shell option.
- `system::virtual::FileBody::Terminal`
    - This is a new variant of `FileBody` that represents a terminal device.
- Private dependencies:
    - yash-executor 1.0.0

### Changed

- The child process created by `system::real::RealSystem::new_child_process`
  now uses a new executor instead of reusing the executor inherited from the
  parent process.
- `system::ChildProcessStarter` has been redefined as
  `Box<dyn FnOnce(&mut Env, ChildProcessTask) -> Pid>`. The function now
  directly returns the PID of the child process instead of a future that
  resolves to the PID.
- `system::virtual::FileBody` is now `non_exhaustive`.
- `system::virtual::VirtualSystem::isatty` now returns true for a file
  descriptor associated with `FileBody::Terminal`.
- `impl yash_syntax::input::Input` for `input::FdReader`, `input::Echo`,
  `input::IgnoreEof`, and `input::Reporter` now conforms to the new definition
  of the `next_line` method.
- Public dependency versions:
    - yash-syntax 0.11.0 → 0.12.0

### Removed

- The type parameter constraint `T: 'a` is removed from the declarations of
  `input::Echo<'a, 'b, T>` and `input::Reporter<'a, 'b, T>`.
- The redundant lifetime constraint `T: 'a` is removed from the implementations
  of `yash_syntax::input::Input` for `input::Echo<'a, 'b, T>` and
  `input::Reporter<'a, 'b, T>`.
- Private dependencies:
    - async-trait 0.1.73

## [0.3.0] - 2024-08-22

### Added

- This crate now builds on non-Unix platforms. However,
  `system::real::RealSystem` is only available on Unix platforms.
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
- The crate now re-exports `unix_path` as `path` and `unix_str` as `str`.
- Public dependencies:
    - enumset 1.1.2 (previously a private dependency)
    - unix_path 1.0.1
    - unix_str 1.0.0
- Private dependencies:
    - bitflags 2.6.0
    - nix 0.29.0 (with the "fs", "signal", and "user" features enabled)

### Changed

- `system::FdFlag` is no longer a re-export of `nix::fcntl::FdFlag`.
- `system::Mode` is no longer a re-export of `nix::sys::stat::Mode`.
- All APIs that handle `std::path::Path`, `std::path::PathBuf`, `std::ffi::OsStr`,
  and `std::ffi::OsString` now use `path::Path`, `path::PathBuf`, `str::OsStr`,
  and `str::OsString` instead.
    - `system::DirEntry::name`
    - `system::System::confstr_path`
    - `system::System::getcwd`
    - `system::System::getpwnam_dir`
    - `system::System::open_tmpfile`
    - `system::virtual::FileBody::Directory::files`
    - `system::virtual::FileBody::Symlink::target`
    - `system::virtual::FileSystem::get`
    - `system::virtual::FileSystem::save`
    - `system::virtual::Process::chdir`
    - `system::virtual::SystemState::home_dirs`
    - `system::virtual::SystemState::path`
    - `system::virtual::VirtualDir::new`
- `system::SignalHandling` has been renamed to `system::Disposition`.
- In the `system::virtual::Process` struct, the following methods have been
  renamed:
    - `signal_handling` → `disposition`
    - `set_signal_handling` → `set_disposition`
- In the `trap::SignalSystem` trait, the `set_signal_handling` method has been
  renamed to `set_disposition`.
- In the `trap::TrapSet` struct, the following methods have been renamed:
    - `enable_sigchld_handler` → `enable_internal_disposition_for_sigchld`
    - `enable_terminator_handlers` → `enable_internal_dispositions_for_terminators`
    - `enable_stopper_handlers` → `enable_internal_dispositions_for_stoppers`
    - `disable_terminator_handlers` → `disable_internal_dispositions_for_terminators`
    - `disable_stopper_handlers` → `disable_internal_dispositions_for_stoppers`
    - `disable_internal_handlers` → `disable_internal_dispositions`
- The `fstat` and `fstatat` methods of `system::System` now return a `Stat`
  instead of a `nix::sys::stat::FileStat`.
- The `system::System::fstatat` method now takes a `follow_symlinks: bool`
  parameter instead of an `AtFlags` parameter.
- The `system::System::open` method has been redefined to take `OfdAccess` and
  `OpenFlag` parameters instead of `nix::fcntl::OFlag`.
- The `system::System::isatty` method now returns a `bool` instead of a
  `system::Result<bool>`.
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
- Public dependency versions:
    - Rust 1.77.0 → 1.79.0
    - yash-syntax 0.10.0 → 0.11.0

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
- `impl From<job::Pid> for nix::unistd::Pid`
- `impl From<nix::unistd::Pid> for job::Pid`
- `impl From<system::resource::LimitPair> for nix::libc::rlimit`
- `impl From<nix::libc::rlimit> for system::resource::LimitPair`
- Public dependencies:
    - nix 0.27.0 (now a private dependency with the "fs", "signal" and "user"
      features enabled)

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

- Public dependency versions:
    - Rust 1.70.0 → 1.77.0
    - yash-syntax 0.9.0 → 0.10.0
- Private dependency versions:
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

- Public dependency versions
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

[0.10.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.10.0
[0.9.2]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.9.2
[0.9.1]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.9.1
[0.9.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.9.0
[0.8.1]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.8.1
[0.8.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.8.0
[0.7.1]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.7.1
[0.7.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.7.0
[0.6.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.6.0
[0.5.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.5.0
[0.4.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.4.0
[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.3.0
[0.2.1]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.2.1
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-env-0.1.0
