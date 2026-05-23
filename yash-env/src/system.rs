// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! API declarations and implementations for system-managed parts of the environment
//!
//! This module defines many traits that declare methods to interact with the
//! underlying system, such as file system operations, process management,
//! signal handling, and resource limit management:
//!
//! - [`CaughtSignals`]: Declares the `caught_signals` method for retrieving
//!   caught signals.
//! - [`Chdir`]: Declares the `chdir` method for changing the current
//!   working directory.
//! - [`Clock`]: Declares the `now` method for getting the current time.
//! - [`Close`]: Declares the `close` method for closing file descriptors.
//! - [`Dup`]: Declares the `dup` and `dup2` methods for duplicating file
//!   descriptors.
//! - [`Exec`]: Declares the `execve` method for executing new programs.
//! - [`Exit`]: Declares the `exit` method for terminating the current
//!   process.
//! - [`Fcntl`]: Declares the `ofd_access`, `get_and_set_nonblocking`,
//!   `fcntl_getfd`, and `fcntl_setfd` methods for `fcntl`-related operations.
//! - [`Fork`]: Declares a method for creating new child processes.
//! - [`Fstat`]: Declares `fstat` and `fstatat` methods for getting file
//!   metadata and provides a default implementation of `is_directory`.
//! - [`GetCwd`]: Declares the `getcwd` method for getting the current
//!   working directory.
//! - [`GetPid`]: Declares the `getpid` and other methods for getting process IDs
//!   and other attributes.
//! - [`GetPw`]: Declares methods for getting user information.
//! - [`GetSigaction`]: Declares the `get_sigaction` method for querying signal
//!   dispositions.
//! - [`GetUid`]: Declares the `getuid`, `geteuid`, `getgid`, and
//!   `getegid` methods for getting user and group IDs.
//! - [`IsExecutableFile`]: Declares the `is_executable_file` method for checking
//!   if a file is executable.
//! - [`Isatty`]: Declares the `isatty` method for testing if a file descriptor is
//!   associated with a terminal device.
//! - [`Open`]: Declares the `open` and other methods for opening files.
//! - [`Pipe`]: Declares the `pipe` method for creating pipes.
//! - [`Read`]: Declares the `read` method for reading from file descriptors.
//! - [`Seek`]: Declares the `lseek` method for seeking within file
//!   descriptors.
//! - [`Select`]: Declares the `select` method for waiting on multiple file
//!   descriptors and signals.
//! - [`SendSignal`]: Declares the `kill` and `raise` methods for sending signals
//!   to processes.
//! - [`SetPgid`]: Declares the `setpgid` method for setting process group IDs.
//! - [`ShellPath`]: Declares the `shell_path` method for getting the path to
//!   the shell executable.
//! - [`Sigaction`]: Declares methods for managing signal dispositions.
//! - [`Sigmask`]: Declares the `sigmask` method for managing signal masks.
//! - [`Signals`]: Declares the `signal_number_from_name` and
//!   `validate_signal` methods for converting between signal names and numbers.
//! - [`TcGetPgrp`]: Declares the `tcgetpgrp` method for getting the
//!   foreground process group ID of a terminal.
//! - [`TcSetPgrp`]: Declares the `tcsetpgrp` method for setting the
//!   foreground process group ID of a terminal.
//! - [`Times`]: Declares the `times` method for getting CPU times.
//! - [`Umask`]: Declares the `umask` method for setting the file mode
//!   creation mask.
//! - [`Wait`]: Declares the `wait` method for waiting for child processes.
//! - [`Write`]: Declares the `write` method for writing to file
//!   descriptors.
//! - [`resource::GetRlimit`]: Declares the `getrlimit` method for
//!   retrieving resource limits.
//! - [`resource::SetRlimit`]: Declares the `setrlimit` method for
//!   setting resource limits.
//!
//! There are two main implementors of these traits:
//!
//! - [`RealSystem`]: An implementation that interacts with the actual
//!   underlying system (see the [`real`] module).
//! - [`VirtualSystem`]: An implementation that simulates system behavior for
//!   testing purposes (see the [`virtual`] module).
//!
//! Additionally, [`Concurrent`] is a wrapper that extends the interface with
//! asynchronous methods for concurrency. (See the [`concurrency`] module.)
//!
//! Some methods of these traits return [futures](std::future::Future), not
//! because the underlying system calls are asynchronous, but to allow
//! `VirtualSystem` to simulate blocking behavior and run virtual processes
//! concurrently. `RealSystem` implementations return ready futures after the
//! underlying system calls complete (which may block the current thread).

pub mod concurrency;
mod errno;
mod file_system;
mod future;
mod io;
mod process;
#[cfg(unix)]
pub mod real;
pub mod resource;
mod select;
mod signal;
mod sysconf;
mod terminal;
mod time;
mod user;
pub mod r#virtual;

pub use self::concurrency::{Concurrent, SignalList};
pub use self::errno::Errno;
pub use self::errno::RawErrno;
pub use self::errno::Result;
pub use self::file_system::{
    AT_FDCWD, Chdir, Dir, DirEntry, FileType, Fstat, GetCwd, IsExecutableFile, Mode, OfdAccess,
    Open, OpenFlag, RawMode, Seek, Stat, Umask,
};
#[allow(deprecated, reason = "for backward compatible API")]
pub use self::future::FlexFuture;
pub use self::io::{Close, Dup, Fcntl, FdFlag, Pipe, Read, Write};
pub use self::process::{Exec, Exit, Fork, GetPid, SetPgid, Wait};
#[cfg(all(doc, unix))]
use self::real::RealSystem;
use self::resource::{GetRlimit, SetRlimit};
pub use self::select::{FdSet, Select};
pub use self::signal::{
    CaughtSignals, Disposition, GetSigaction, SendSignal, Sigaction, Sigmask, SigmaskOp, Signals,
    Sigset,
};
pub use self::sysconf::{ShellPath, Sysconf};
pub use self::terminal::{Isatty, TcGetPgrp, TcSetPgrp};
pub use self::time::{Clock, CpuTimes, Times};
pub use self::user::{GetPw, GetUid, Gid, RawGid, RawUid, Uid};
#[cfg(doc)]
use self::r#virtual::VirtualSystem;
use crate::io::Fd;
use crate::job::Pid;
use crate::semantics::ExitStatus;
