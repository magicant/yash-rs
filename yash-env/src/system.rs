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
//! This module defines the [`System`] trait, which provides an interface to
//! interact with the underlying system. It is a subtrait of various other traits
//! that define specific functionalities, such as file system operations, process
//! management, signal handling, and resource limit management. The following
//! traits are included as subtraits of `System`:
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
//! There are two main implementors of the `System` trait:
//!
//! - `RealSystem`: An implementation that interacts with the actual
//!   underlying system (see the [`real`] module).
//! - `VirtualSystem`: An implementation that simulates system behavior
//!   for testing purposes (see the [`virtual`] module).
//!
//! Additionally, there is the [`SharedSystem`] implementor that wraps
//! another `System` instance to provide asynchronous methods.
//!
//! User code should generally depend only on specific subtraits of `System`
//! rather than `System` itself. This allows for more modular and testable code.
//! For example, code that only needs to write to file descriptors can depend
//! on the `Write` trait alone.

mod errno;
mod file_system;
mod future;
mod io;
mod process;
#[cfg(unix)]
pub mod real;
pub mod resource;
mod select;
mod shared;
mod signal;
mod sysconf;
mod terminal;
mod time;
mod user;
pub mod r#virtual;

pub use self::errno::Errno;
pub use self::errno::RawErrno;
pub use self::errno::Result;
pub use self::file_system::{
    AT_FDCWD, Chdir, Dir, DirEntry, FileType, Fstat, GetCwd, IsExecutableFile, Mode, OfdAccess,
    Open, OpenFlag, RawMode, Seek, Stat, Umask,
};
pub use self::future::FlexFuture;
pub use self::io::{Close, Dup, Fcntl, FdFlag, Pipe, Read, Write};
pub use self::process::{
    ChildProcessStarter, ChildProcessTask, Exec, Exit, Fork, GetPid, SetPgid, Wait,
};
#[cfg(all(doc, unix))]
use self::real::RealSystem;
use self::resource::{GetRlimit, LimitPair, Resource, SetRlimit};
pub use self::select::Select;
use self::select::SelectSystem;
use self::select::SignalStatus;
pub use self::shared::SharedSystem;
pub use self::signal::{
    CaughtSignals, Disposition, SendSignal, Sigaction, Sigmask, SigmaskOp, Signals,
};
pub use self::sysconf::{ShellPath, Sysconf};
pub use self::terminal::{Isatty, TcGetPgrp, TcSetPgrp};
pub use self::time::{Clock, CpuTimes, Times};
pub use self::user::{GetPw, GetUid, Gid, RawGid, RawUid, Uid};
#[cfg(doc)]
use self::r#virtual::VirtualSystem;
use crate::io::Fd;
use crate::job::Pid;
use crate::path::Path;
use crate::path::PathBuf;
use crate::semantics::ExitStatus;
use crate::str::UnixString;
#[cfg(doc)]
use crate::subshell::Subshell;
use crate::trap::SignalSystem;
use std::fmt::Debug;

/// API to the system-managed parts of the environment.
///
/// The `System` trait defines a collection of methods to access the underlying
/// operating system from the shell as an application program. There are two
/// substantial implementors for this trait: [`RealSystem`] and
/// [`VirtualSystem`]. Another implementor is [`SharedSystem`], which wraps a
/// `System` instance to extend the interface with asynchronous methods.
pub trait System:
    CaughtSignals
    + Chdir
    + Clock
    + Close
    + Debug
    + Dup
    + Exec
    + Exit
    + Fcntl
    + Fork
    + Fstat
    + GetCwd
    + GetPid
    + GetPw
    + GetRlimit
    + GetUid
    + IsExecutableFile
    + Isatty
    + Open
    + Pipe
    + Read
    + Seek
    + Select
    + SendSignal
    + SetPgid
    + SetRlimit
    + ShellPath
    + Sigaction
    + Sigmask
    + Signals
    + Sysconf
    + TcGetPgrp
    + TcSetPgrp
    + Times
    + Umask
    + Wait
    + Write
{
}

impl<T> System for T where
    T: CaughtSignals
        + Chdir
        + Clock
        + Close
        + Debug
        + Dup
        + Exec
        + Exit
        + Fcntl
        + Fork
        + Fstat
        + GetCwd
        + GetPid
        + GetPw
        + GetRlimit
        + GetUid
        + IsExecutableFile
        + Isatty
        + Open
        + Pipe
        + Read
        + Seek
        + Select
        + SendSignal
        + SetPgid
        + SetRlimit
        + ShellPath
        + Sigaction
        + Sigmask
        + Signals
        + Sysconf
        + TcGetPgrp
        + TcSetPgrp
        + Times
        + Umask
        + Wait
        + Write
{
}

/// Extension for [`System`]
///
/// This trait provides some extension methods for `System`.
pub trait SystemEx: System {}

impl<T: System + ?Sized> SystemEx for T {}
