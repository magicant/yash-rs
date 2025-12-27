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
mod id;
mod io;
#[cfg(unix)]
pub mod real;
pub mod resource;
mod select;
mod shared;
mod signal;
mod time;
pub mod r#virtual;

pub use self::errno::Errno;
pub use self::errno::RawErrno;
pub use self::errno::Result;
pub use self::file_system::{
    AT_FDCWD, Dir, DirEntry, FileType, Fstat, IsExecutableFile, Mode, OfdAccess, Open, OpenFlag,
    RawMode, Seek, Stat, Umask,
};
pub use self::future::FlexFuture;
pub use self::id::Gid;
pub use self::id::RawGid;
pub use self::id::RawUid;
pub use self::id::Uid;
pub use self::io::FdFlag;
pub use self::io::{Close, Dup, Fcntl, Pipe, Read, Write};
#[cfg(all(doc, unix))]
use self::real::RealSystem;
use self::resource::LimitPair;
use self::resource::Resource;
use self::select::SelectSystem;
use self::select::SignalStatus;
pub use self::shared::SharedSystem;
pub use self::signal::{Disposition, Sigaction, Signals};
pub use self::time::{CpuTimes, Time, Times};
#[cfg(doc)]
use self::r#virtual::VirtualSystem;
use crate::Env;
use crate::io::Fd;
use crate::io::MIN_INTERNAL_FD;
use crate::job::Pid;
use crate::job::ProcessState;
use crate::path::Path;
use crate::path::PathBuf;
use crate::semantics::ExitStatus;
use crate::str::UnixString;
#[cfg(doc)]
use crate::subshell::Subshell;
use crate::trap::SignalSystem;
use std::convert::Infallible;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::c_int;
use std::fmt::Debug;
use std::pin::Pin;
use std::time::Duration;
use r#virtual::SignalEffect;

/// API to the system-managed parts of the environment.
///
/// The `System` trait defines a collection of methods to access the underlying
/// operating system from the shell as an application program. There are two
/// substantial implementors for this trait: [`RealSystem`] and
/// [`VirtualSystem`]. Another implementor is [`SharedSystem`], which wraps a
/// `System` instance to extend the interface with asynchronous methods.
pub trait System:
    Close
    + Debug
    + Dup
    + Fcntl
    + Fstat
    + IsExecutableFile
    + Open
    + Pipe
    + Read
    + Seek
    + Sigaction
    + Signals
    + Time
    + Times
    + Umask
    + Write
{
    /// Tests if a file descriptor is associated with a terminal device.
    ///
    /// On error, this function simply returns `false` and no detailed error
    /// information is provided because POSIX does not require the `isatty`
    /// function to set `errno`.
    fn isatty(&self, fd: Fd) -> bool;

    /// Gets and/or sets the signal blocking mask.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::set_disposition`]. You should not call this function
    /// directly, or you will disrupt the behavior of `SharedSystem`. The
    /// description below applies if you want to do everything yourself without
    /// depending on `SharedSystem`.
    ///
    /// This is a thin wrapper around the `sigprocmask` system call. If `op` is
    /// `Some`, this function updates the signal blocking mask by applying the
    /// given `SigmaskOp` and signal set to the current mask. If `op` is `None`,
    /// this function does not change the mask.
    /// If `old_mask` is `Some`, this function sets the previous mask to it.
    fn sigmask(
        &mut self,
        op: Option<(SigmaskOp, &[signal::Number])>,
        old_mask: Option<&mut Vec<signal::Number>>,
    ) -> Result<()>;

    /// Returns signals this process has caught, if any.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::select`]. You should not call this function directly, or
    /// you will disrupt the behavior of `SharedSystem`. The description below
    /// applies if you want to do everything yourself without depending on
    /// `SharedSystem`.
    ///
    /// To catch a signal, you must firstly install a signal handler by calling
    /// [`Sigaction::sigaction`] with [`Disposition::Catch`]. Once the handler
    /// is ready, signals sent to the process are accumulated in the `System`.
    /// You call `caught_signals` to obtain a list of caught signals thus far.
    ///
    /// This function clears the internal list of caught signals, so a next call
    /// will return an empty list unless another signal is caught since the
    /// first call. Because the list size is limited, you should call this
    /// function periodically before the list gets full, in which case further
    /// caught signals are silently ignored.
    ///
    /// Note that signals become pending if sent while blocked by
    /// [`sigmask`](Self::sigmask). They must be unblocked so that they are
    /// caught and made available from this function.
    fn caught_signals(&mut self) -> Vec<signal::Number>;

    /// Sends a signal.
    ///
    /// This is a thin wrapper around the `kill` system call. If `signal` is
    /// `None`, permission to send a signal is checked, but no signal is sent.
    ///
    /// The virtual system version of this function blocks the calling thread if
    /// the signal stops or terminates the current process, hence returning a
    /// future. See [`VirtualSystem::kill`] for details.
    fn kill(&mut self, target: Pid, signal: Option<signal::Number>) -> FlexFuture<Result<()>>;

    /// Sends a signal to the current process.
    ///
    /// This is a thin wrapper around the `raise` system call.
    ///
    /// The virtual system version of this function blocks the calling thread if
    /// the signal stops or terminates the current process, hence returning a
    /// future. See [`VirtualSystem::kill`] for details.
    fn raise(&mut self, signal: signal::Number) -> FlexFuture<Result<()>>;

    /// Waits for a next event.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::select`]. You should not call this function directly, or
    /// you will disrupt the behavior of `SharedSystem`. The description below
    /// applies if you want to do everything yourself without depending on
    /// `SharedSystem`.
    ///
    /// This function blocks the calling thread until one of the following
    /// condition is met:
    ///
    /// - An FD in `readers` becomes ready for reading.
    /// - An FD in `writers` becomes ready for writing.
    /// - The specified `timeout` duration has passed.
    /// - A signal handler catches a signal.
    ///
    /// When this function returns an `Ok`, FDs that are not ready for reading
    /// and writing are removed from `readers` and `writers`, respectively. The
    /// return value will be the number of FDs left in `readers` and `writers`.
    ///
    /// If `readers` and `writers` contain an FD that is not open for reading
    /// and writing, respectively, this function will fail with `EBADF`. In this
    /// case, you should remove the FD from `readers` and `writers` and try
    /// again.
    ///
    /// If `signal_mask` is `Some` list of signals, it is used as the signal
    /// blocking mask while waiting and restored when the function returns.
    fn select(
        &mut self,
        readers: &mut Vec<Fd>,
        writers: &mut Vec<Fd>,
        timeout: Option<Duration>,
        signal_mask: Option<&[signal::Number]>,
    ) -> Result<c_int>;

    /// Returns the session ID of the specified process.
    ///
    /// If `pid` is `Pid(0)`, this function returns the session ID of the
    /// current process.
    fn getsid(&self, pid: Pid) -> Result<Pid>;

    /// Returns the process ID of the current process.
    #[must_use]
    fn getpid(&self) -> Pid;

    /// Returns the process ID of the parent process.
    #[must_use]
    fn getppid(&self) -> Pid;

    /// Returns the process group ID of the current process.
    #[must_use]
    fn getpgrp(&self) -> Pid;

    /// Modifies the process group ID of a process.
    ///
    /// This is a thin wrapper around the `setpgid` system call.
    fn setpgid(&mut self, pid: Pid, pgid: Pid) -> Result<()>;

    /// Returns the current foreground process group ID.
    ///
    /// This is a thin wrapper around the `tcgetpgrp` system call.
    fn tcgetpgrp(&self, fd: Fd) -> Result<Pid>;

    /// Switches the foreground process group.
    ///
    /// This is a thin wrapper around the `tcsetpgrp` system call.
    ///
    /// The virtual system version of this function may block the calling thread
    /// if called in a background process group, hence returning a future.
    fn tcsetpgrp(&mut self, fd: Fd, pgid: Pid) -> FlexFuture<Result<()>>;

    /// Creates a new child process.
    ///
    /// This is a thin wrapper around the `fork` system call. Users of `Env`
    /// should not call it directly. Instead, use [`Subshell`] so that the
    /// environment can condition the state of the child process before it
    /// starts running.
    ///
    /// Because we need the parent environment to create the child environment,
    /// this method cannot initiate the child task directly. Instead, it returns
    /// a [`ChildProcessStarter`] function that takes the parent environment and
    /// the child task. The caller must call the starter to make sure the parent
    /// and child processes perform correctly after forking.
    fn new_child_process(&mut self) -> Result<ChildProcessStarter<Self>>
    where
        Self: Sized;

    /// Reports updated status of a child process.
    ///
    /// This is a low-level function used internally by
    /// [`Env::wait_for_subshell`]. You should not call this function directly,
    /// or you will disrupt the behavior of `Env`. The description below applies
    /// if you want to do everything yourself without depending on `Env`.
    ///
    /// This function performs
    /// `waitpid(target, ..., WUNTRACED | WCONTINUED | WNOHANG)`.
    /// Despite the name, this function does not block: it returns the result
    /// immediately.
    ///
    /// This function returns a pair of the process ID and the process state if
    /// a process matching `target` is found and its state has changed. If all
    /// the processes matching `target` have not changed their states, this
    /// function returns `Ok(None)`. If an error occurs, this function returns
    /// `Err(_)`.
    fn wait(&mut self, target: Pid) -> Result<Option<(Pid, ProcessState)>>;

    // TODO Consider passing raw pointers for optimization
    /// Replaces the current process with an external utility.
    ///
    /// This is a thin wrapper around the `execve` system call.
    fn execve(
        &mut self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> FlexFuture<Result<Infallible>>;

    /// Terminates the current process.
    ///
    /// This function is a thin wrapper around the `_exit` system call.
    fn exit(&mut self, exit_status: ExitStatus) -> FlexFuture<Infallible>;

    /// Returns the current working directory path.
    fn getcwd(&self) -> Result<PathBuf>;

    /// Changes the working directory.
    fn chdir(&mut self, path: &CStr) -> Result<()>;

    /// Returns the real user ID of the current process.
    fn getuid(&self) -> Uid;

    /// Returns the effective user ID of the current process.
    fn geteuid(&self) -> Uid;

    /// Returns the real group ID of the current process.
    fn getgid(&self) -> Gid;

    /// Returns the effective group ID of the current process.
    fn getegid(&self) -> Gid;

    /// Returns the home directory path of the given user.
    ///
    /// Returns `Ok(None)` if the user is not found.
    fn getpwnam_dir(&self, name: &CStr) -> Result<Option<PathBuf>>;

    /// Returns the standard `$PATH` value where all standard utilities are
    /// expected to be found.
    ///
    /// This is a thin wrapper around the `confstr(_CS_PATH, …)`.
    fn confstr_path(&self) -> Result<UnixString>;

    /// Returns the path to the shell executable.
    ///
    /// If possible, this function should return the path to the current shell
    /// executable. Otherwise, it should return the path to the default POSIX
    /// shell.
    fn shell_path(&self) -> CString;

    /// Returns the limits for the specified resource.
    ///
    /// This function returns a pair of the soft and hard limits for the given
    /// resource. The soft limit is the current limit, and the hard limit is the
    /// maximum value that the soft limit can be set to.
    ///
    /// When no limit is set, the limit value is [`INFINITY`].
    ///
    /// This is a thin wrapper around the `getrlimit` system call.
    ///
    /// [`INFINITY`]: self::resource::INFINITY
    fn getrlimit(&self, resource: Resource) -> Result<LimitPair>;

    /// Sets the limits for the specified resource.
    ///
    /// Specify [`INFINITY`] as the limit value to remove the limit.
    ///
    /// This is a thin wrapper around the `setrlimit` system call.
    ///
    /// [`INFINITY`]: self::resource::INFINITY
    fn setrlimit(&mut self, resource: Resource, limits: LimitPair) -> Result<()>;
}

/// Operation applied to the signal blocking mask
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum SigmaskOp {
    /// Add signals to the mask (`SIG_BLOCK`)
    Add,
    /// Remove signals from the mask (`SIG_UNBLOCK`)
    Remove,
    /// Set the mask to the given signals (`SIG_SETMASK`)
    Set,
}

/// Task executed in a child process
///
/// This is an argument passed to a [`ChildProcessStarter`]. The task is
/// executed in a child process initiated by the starter. The environment passed
/// to the task is a clone of the parent environment, but it has a different
/// process ID than the parent.
///
/// Note that the output type of the task is `Infallible`. This is to ensure that
/// the task [exits](System::exit) cleanly or [kills](System::kill) itself with
/// a signal.
pub type ChildProcessTask<S> =
    Box<dyn for<'a> FnOnce(&'a mut Env<S>) -> Pin<Box<dyn Future<Output = Infallible> + 'a>>>;

/// Abstract function that starts a child process
///
/// [`System::new_child_process`] returns a child process starter. You need to
/// pass the parent environment and a task to the starter to complete the child
/// process creation. The starter provides a unified interface that hides the
/// differences between [`RealSystem`] and [`VirtualSystem`].
///
/// [`RealSystem::new_child_process`] performs a `fork` system call and returns
/// a starter in the parent and child processes. When the starter is called in
/// the parent, it just returns the child process ID. The starter in the child
/// process runs the task and exits the process with the exit status of the
/// task.
///
/// [`VirtualSystem::new_child_process`] does ont create a real child process.
/// Instead, the starter runs the task concurrently in the current process using
/// the executor contained in the system. A new [`Process`](virtual::Process) is
/// added to the system to represent the child process. The starter returns its
/// process ID.
///
/// This function only starts the child, which continues to run asynchronously
/// after the function returns its PID. To wait for the child to finish and
/// obtain its exit status, use [`System::wait`].
pub type ChildProcessStarter<S> = Box<dyn FnOnce(&mut Env<S>, ChildProcessTask<S>) -> Pid>;

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
    /// safely. If you call [`tcsetpgrp`](System::tcsetpgrp) from a background
    /// process, the process is stopped by SIGTTOU by default. To prevent this
    /// effect, SIGTTOU must be blocked or ignored when `tcsetpgrp` is called.
    /// This function uses [`sigmask`](System::sigmask) to block SIGTTOU before
    /// calling [`tcsetpgrp`](System::tcsetpgrp) and also to restore the
    /// original signal mask after `tcsetpgrp`.
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
    /// process), [`sigmask`](System::sigmask) to unblock SIGTTOU, and
    /// [`tcsetpgrp`](System::tcsetpgrp) to modify the foreground job. If the
    /// calling process is not in the foreground, `tcsetpgrp` will suspend the
    /// process with SIGTTOU until another job-controlling process resumes it in
    /// the foreground. After `tcsetpgrp` completes, this function calls
    /// `sigmask` and `sigaction` to restore the original state.
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
