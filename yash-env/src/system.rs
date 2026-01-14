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
#[cfg(doc)]
use crate::io::MIN_INTERNAL_FD;
use crate::job::Pid;
use crate::path::Path;
use crate::path::PathBuf;
use crate::semantics::ExitStatus;
use crate::str::UnixString;
#[cfg(doc)]
use crate::subshell::Subshell;
use crate::trap::SignalSystem;
use std::convert::Infallible;
use std::fmt::Debug;

/// API to the system-managed parts of the environment.
///
/// The `System` trait defines a collection of methods to access the underlying
/// operating system from the shell as an application program. There are two
/// substantial implementors for this trait: [`RealSystem`] and
/// [`VirtualSystem`]. Another implementor is [`SharedSystem`], which wraps a
/// `System` instance to extend the interface with asynchronous methods.
#[deprecated(
    note = "use smaller, more specialized traits declared in the `system` module instead",
    since = "0.11.0"
)]
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

#[allow(deprecated)]
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
#[allow(deprecated)]
#[deprecated(
    note = "use functions in the `yash-env::io` and `yash-env::job` modules instead",
    since = "0.11.0"
)]
pub trait SystemEx: System {
    /// Moves a file descriptor to [`MIN_INTERNAL_FD`] or larger.
    ///
    /// This function can be used to make sure a file descriptor used by the
    /// shell does not conflict with file descriptors used by the user.
    /// [`MIN_INTERNAL_FD`] is the minimum file descriptor number the shell
    /// uses internally. This function moves the file descriptor to a number
    /// larger than or equal to [`MIN_INTERNAL_FD`].
    ///
    /// If the given file descriptor is less than [`MIN_INTERNAL_FD`], this
    /// function duplicates the file descriptor with [`Dup::dup`] and closes
    /// the original one. Otherwise, this function does nothing.
    ///
    /// The new file descriptor will have the CLOEXEC flag set when it is
    /// dupped. Note that, if the original file descriptor has the CLOEXEC flag
    /// unset and is already larger than or equal to [`MIN_INTERNAL_FD`], this
    /// function will not set the CLOEXEC flag for the returned file descriptor.
    ///
    /// This function returns the new file descriptor on success. On error, it
    /// closes the original file descriptor and returns the error.
    #[deprecated(
        note = "use `yash_env::io::move_fd_internal` instead",
        since = "0.11.0"
    )]
    fn move_fd_internal(&mut self, from: Fd) -> Result<Fd> {
        crate::io::move_fd_internal(self, from)
    }

    /// Tests if a file descriptor is a pipe.
    #[deprecated(
        note = "use `yash_env::system::Fstat::fd_is_pipe` instead",
        since = "0.11.0"
    )]
    fn fd_is_pipe(&self, fd: Fd) -> bool {
        self.fstat(fd)
            .is_ok_and(|stat| stat.r#type == FileType::Fifo)
    }

    /// Switches the foreground process group with SIGTTOU blocked.
    ///
    /// This is a convenience function to change the foreground process group
    /// safely. If you call [`TcSetPgrp::tcsetpgrp`] from a background process,
    /// the process is stopped by SIGTTOU by default. To prevent this effect,
    /// SIGTTOU must be blocked or ignored when `tcsetpgrp` is called.  This
    /// function uses [`Sigmask::sigmask`] to block SIGTTOU before calling
    /// `tcsetpgrp` and also to restore the original signal mask after
    /// `tcsetpgrp`.
    ///
    /// Use [`tcsetpgrp_without_block`](Self::tcsetpgrp_without_block) if you
    /// need to make sure the shell is in the foreground before changing the
    /// foreground job.
    #[deprecated(
        note = "use `yash_env::job::tcsetpgrp_with_block` instead",
        since = "0.11.0"
    )]
    fn tcsetpgrp_with_block(&mut self, fd: Fd, pgid: Pid) -> impl Future<Output = Result<()>> {
        crate::job::tcsetpgrp_with_block(self, fd, pgid)
    }

    /// Switches the foreground process group with the default SIGTTOU settings.
    ///
    /// This is a convenience function to ensure the shell has been in the
    /// foreground and optionally change the foreground process group. This
    /// function calls [`Sigaction::sigaction`] to restore the action for
    /// SIGTTOU to the default disposition (which is to suspend the shell
    /// process), [`Sigmask::sigmask`] to unblock SIGTTOU, and
    /// [`TcSetPgrp::tcsetpgrp`] to modify the foreground job. If the calling
    /// process is not in the foreground, `tcsetpgrp` will suspend the process
    /// with SIGTTOU until another job-controlling process resumes it in the
    /// foreground. After `tcsetpgrp` completes, this function calls `sigmask`
    /// and `sigaction` to restore the original state.
    ///
    /// Note that if `pgid` is the process group ID of the current process, this
    /// function does not change the foreground job, but the process is still
    /// subject to suspension if it has not been in the foreground.
    ///
    /// Use [`tcsetpgrp_with_block`](Self::tcsetpgrp_with_block) to change the
    /// job even if the current shell is not in the foreground.
    #[deprecated(
        note = "use `yash_env::job::tcsetpgrp_without_block` instead",
        since = "0.11.0"
    )]
    fn tcsetpgrp_without_block(&mut self, fd: Fd, pgid: Pid) -> impl Future<Output = Result<()>> {
        crate::job::tcsetpgrp_without_block(self, fd, pgid)
    }

    /// Returns the signal name for the signal number.
    ///
    /// This function returns the signal name for the given signal number.
    ///
    /// If the signal number is invalid, this function panics. It may occur if
    /// the number is from a different system or was created without checking
    /// the validity.
    #[deprecated(
        note = "use `yash_env::system::Signals::signal_name_from_number` instead",
        since = "0.11.0"
    )]
    #[must_use]
    fn signal_name_from_number(&self, number: signal::Number) -> signal::Name {
        self.validate_signal(number.as_raw()).unwrap().0
    }

    /// Terminates the current process with the given exit status, possibly
    /// sending a signal to kill the process.
    ///
    /// If the exit status represents a signal that killed the last executed
    /// command, this function sends the signal to the current process to
    /// propagate the signal to the parent process. Otherwise, this function
    /// terminates the process with the given exit status.
    #[deprecated(
        note = "use `yash_env::semantics::exit_or_raise` instead",
        since = "0.11.0"
    )]
    fn exit_or_raise(&mut self, exit_status: ExitStatus) -> impl Future<Output = Infallible> {
        async move { crate::semantics::exit_or_raise(self, exit_status).await }
    }
}

#[allow(deprecated)]
impl<T: System + ?Sized> SystemEx for T {}
