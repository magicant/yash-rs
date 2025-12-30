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

//! [System] and its implementors.

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
pub use self::time::{CpuTimes, Time, Times};
pub use self::user::{GetPw, GetUid, Gid, RawGid, RawUid, Uid};
#[cfg(doc)]
use self::r#virtual::VirtualSystem;
use crate::io::Fd;
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
use r#virtual::SignalEffect;

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
    + Time
    + Times
    + Umask
    + Wait
    + Write
{
}

/// Extension for [`System`]
///
/// This trait provides some extension methods for `System`.
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
    fn move_fd_internal(&mut self, from: Fd) -> Result<Fd> {
        if from >= MIN_INTERNAL_FD {
            return Ok(from);
        }

        let new = self.dup(from, MIN_INTERNAL_FD, FdFlag::CloseOnExec.into());
        self.close(from).ok();
        new
    }

    /// Tests if a file descriptor is a pipe.
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
    fn tcsetpgrp_with_block(&mut self, fd: Fd, pgid: Pid) -> impl Future<Output = Result<()>> {
        async move {
            let sigttou = self
                .signal_number_from_name(signal::Name::Ttou)
                .ok_or(Errno::EINVAL)?;
            let mut old_mask = Vec::new();

            self.sigmask(Some((SigmaskOp::Add, &[sigttou])), Some(&mut old_mask))?;

            let result = self.tcsetpgrp(fd, pgid).await;

            let result_2 = self.sigmask(Some((SigmaskOp::Set, &old_mask)), None);

            result.and(result_2)
        }
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
    fn tcsetpgrp_without_block(&mut self, fd: Fd, pgid: Pid) -> impl Future<Output = Result<()>> {
        async move {
            let sigttou = self
                .signal_number_from_name(signal::Name::Ttou)
                .ok_or(Errno::EINVAL)?;
            match self.sigaction(sigttou, Disposition::Default) {
                Err(e) => Err(e),
                Ok(old_handling) => {
                    let mut old_mask = Vec::new();
                    let result = match self
                        .sigmask(Some((SigmaskOp::Remove, &[sigttou])), Some(&mut old_mask))
                    {
                        Err(e) => Err(e),
                        Ok(()) => {
                            let result = self.tcsetpgrp(fd, pgid).await;

                            let result_2 = self.sigmask(Some((SigmaskOp::Set, &old_mask)), None);

                            result.and(result_2)
                        }
                    };

                    let result_2 = self.sigaction(sigttou, old_handling).map(drop);

                    result.and(result_2)
                }
            }
        }
    }

    /// Returns the signal name for the signal number.
    ///
    /// This function returns the signal name for the given signal number.
    ///
    /// If the signal number is invalid, this function panics. It may occur if
    /// the number is from a different system or was created without checking
    /// the validity.
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
    fn exit_or_raise(&mut self, exit_status: ExitStatus) -> impl Future<Output = Infallible> {
        async fn maybe_raise<S: System + ?Sized>(
            exit_status: ExitStatus,
            system: &mut S,
        ) -> Option<Infallible> {
            let signal = exit_status.to_signal(system, /* exact */ true)?;

            if !matches!(SignalEffect::of(signal.0), SignalEffect::Terminate { .. }) {
                return None;
            }

            // Disable core dump
            system
                .setrlimit(Resource::CORE, LimitPair { soft: 0, hard: 0 })
                .ok()?;

            if signal.0 != signal::Name::Kill {
                // Reset signal disposition
                system.sigaction(signal.1, Disposition::Default).ok()?;
            }

            // Unblock the signal
            system
                .sigmask(Some((SigmaskOp::Remove, &[signal.1])), None)
                .ok()?;

            // Send the signal to the current process
            system.raise(signal.1).await.ok()?;

            None
        }

        async move {
            maybe_raise(exit_status, self).await;
            self.exit(exit_status).await
        }
    }
}

impl<T: System + ?Sized> SystemEx for T {}
