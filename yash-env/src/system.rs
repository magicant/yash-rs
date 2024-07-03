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
pub mod fd_set;
pub mod real;
pub mod resource;
mod select;
pub mod r#virtual;

pub use self::errno::Errno;
pub use self::errno::RawErrno;
pub use self::errno::Result;
use self::fd_set::FdSet;
#[cfg(doc)]
use self::r#virtual::VirtualSystem;
#[cfg(doc)]
use self::real::RealSystem;
use self::resource::LimitPair;
use self::resource::Resource;
use self::select::SelectSystem;
use self::select::SignalStatus;
use crate::io::Fd;
use crate::io::MIN_INTERNAL_FD;
use crate::job::Pid;
use crate::job::ProcessState;
use crate::semantics::ExitStatus;
use crate::signal;
#[cfg(doc)]
use crate::subshell::Subshell;
use crate::trap::SignalSystem;
use crate::Env;
use futures_util::future::poll_fn;
use futures_util::task::Poll;
#[doc(no_inline)]
pub use nix::fcntl::AtFlags;
#[doc(no_inline)]
pub use nix::fcntl::FdFlag;
#[doc(no_inline)]
pub use nix::fcntl::OFlag;
#[doc(no_inline)]
pub use nix::sys::signal::SigmaskHow;
#[doc(no_inline)]
pub use nix::sys::stat::{FileStat, Mode, SFlag};
#[doc(no_inline)]
pub use nix::sys::time::TimeSpec;
use std::cell::RefCell;
use std::convert::Infallible;
use std::ffi::c_int;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fmt::Debug;
use std::future::Future;
use std::io::SeekFrom;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Instant;

/// API to the system-managed parts of the environment.
///
/// The `System` trait defines a collection of methods to access the underlying
/// operating system from the shell as an application program. There are two
/// substantial implementors for this trait: [`RealSystem`] and
/// [`VirtualSystem`]. Another implementor is [`SharedSystem`], which wraps a
/// `System` instance to extend the interface with asynchronous methods.
pub trait System: Debug {
    /// Retrieves metadata of a file.
    fn fstat(&self, fd: Fd) -> Result<FileStat>;

    /// Retrieves metadata of a file.
    fn fstatat(&self, dir_fd: Fd, path: &CStr, flags: AtFlags) -> Result<FileStat>;

    /// Whether there is an executable file at the specified path.
    #[must_use]
    fn is_executable_file(&self, path: &CStr) -> bool;

    /// Whether there is a directory at the specified path.
    #[must_use]
    fn is_directory(&self, path: &CStr) -> bool;

    /// Creates an unnamed pipe.
    ///
    /// This is a thin wrapper around the `pipe` system call.
    /// If successful, returns the reading and writing ends of the pipe.
    fn pipe(&mut self) -> Result<(Fd, Fd)>;

    /// Duplicates a file descriptor.
    ///
    /// This is a thin wrapper around the `fcntl` system call that opens a new
    /// FD that shares the open file description with `from`. The new FD will be
    /// the minimum unused FD not less than `to_min`. The `flags` are set to the
    /// new FD.
    ///
    /// If successful, returns `Ok(new_fd)`. On error, returns `Err(_)`.
    fn dup(&mut self, from: Fd, to_min: Fd, flags: FdFlag) -> Result<Fd>;

    /// Duplicates a file descriptor.
    ///
    /// This is a thin wrapper around the `dup2` system call. If successful,
    /// returns `Ok(to)`. On error, returns `Err(_)`.
    fn dup2(&mut self, from: Fd, to: Fd) -> Result<Fd>;

    /// Opens a file descriptor.
    ///
    /// This is a thin wrapper around the `open` system call.
    fn open(&mut self, path: &CStr, option: OFlag, mode: Mode) -> Result<Fd>;

    /// Opens a file descriptor associated with an anonymous temporary file.
    ///
    /// This function works similarly to the `O_TMPFILE` flag specified to the
    /// `open` function.
    fn open_tmpfile(&mut self, parent_dir: &Path) -> Result<Fd>;

    /// Closes a file descriptor.
    ///
    /// This is a thin wrapper around the `close` system call.
    ///
    /// This function returns `Ok(())` when the FD is already closed.
    fn close(&mut self, fd: Fd) -> Result<()>;

    /// Returns the file status flags for the open file description.
    ///
    /// This is a thin wrapper around the `fcntl` system call.
    fn fcntl_getfl(&self, fd: Fd) -> Result<OFlag>;

    /// Sets the file status flags for the open file description.
    ///
    /// This is a thin wrapper around the `fcntl` system call.
    fn fcntl_setfl(&mut self, fd: Fd, flags: OFlag) -> Result<()>;

    /// Returns the attributes for the file descriptor.
    ///
    /// This is a thin wrapper around the `fcntl` system call.
    fn fcntl_getfd(&self, fd: Fd) -> Result<FdFlag>;

    /// Sets attributes for the file descriptor.
    ///
    /// This is a thin wrapper around the `fcntl` system call.
    fn fcntl_setfd(&mut self, fd: Fd, flags: FdFlag) -> Result<()>;

    /// Tests if a file descriptor is associated with a terminal device.
    fn isatty(&self, fd: Fd) -> Result<bool>;

    /// Reads from the file descriptor.
    ///
    /// This is a thin wrapper around the `read` system call.
    /// If successful, returns the number of bytes read.
    ///
    /// This function may perform blocking I/O, especially if the `O_NONBLOCK`
    /// flag is not set for the FD. Use [`SharedSystem::read_async`] to support
    /// concurrent I/O in an `async` function context.
    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> Result<usize>;

    /// Writes to the file descriptor.
    ///
    /// This is a thin wrapper around the `write` system call.
    /// If successful, returns the number of bytes written.
    ///
    /// This function may write only part of the `buffer` and block if the
    /// `O_NONBLOCK` flag is not set for the FD. Use [`SharedSystem::write_all`]
    /// to support concurrent I/O in an `async` function context and ensure the
    /// whole `buffer` is written.
    fn write(&mut self, fd: Fd, buffer: &[u8]) -> Result<usize>;

    /// Moves the position of the open file description.
    fn lseek(&mut self, fd: Fd, position: SeekFrom) -> Result<u64>;

    /// Opens a directory for enumerating entries.
    fn fdopendir(&mut self, fd: Fd) -> Result<Box<dyn Dir>>;

    /// Opens a directory for enumerating entries.
    fn opendir(&mut self, path: &CStr) -> Result<Box<dyn Dir>>;

    /// Gets and sets the file creation mode mask.
    ///
    /// This is a thin wrapper around the `umask` system call. It sets the mask
    /// to the given value and returns the previous mask.
    ///
    /// You cannot tell the current mask without setting a new one. If you only
    /// want to get the current mask, you need to set it back to the original
    /// value after getting it.
    fn umask(&mut self, mask: Mode) -> Mode;

    /// Returns the current time.
    #[must_use]
    fn now(&self) -> Instant;

    /// Returns consumed CPU times.
    fn times(&self) -> Result<Times>;

    /// Tests if a signal number is valid.
    ///
    /// This function returns `Some((name, number))` if the signal number refers
    /// to a valid signal supported by the system. Otherwise, it returns `None`.
    ///
    /// Note that one signal number can have multiple names, in which case this
    /// function returns the name that is considered the most common.
    #[must_use]
    fn validate_signal(&self, number: signal::RawNumber) -> Option<(signal::Name, signal::Number)>;

    /// Gets the signal number from the signal name.
    ///
    /// This function returns the signal number corresponding to the signal name
    /// in the system. If the signal name is not supported, it returns `None`.
    #[must_use]
    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number>;

    /// Gets and/or sets the signal blocking mask.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::set_signal_handling`]. You should not call this function
    /// directly, or you will disrupt the behavior of `SharedSystem`. The
    /// description below applies if you want to do everything yourself without
    /// depending on `SharedSystem`.
    ///
    /// This is a thin wrapper around the `sigprocmask` system call. If `op` is
    /// `Some`, this function updates the signal blocking mask by applying the
    /// given `SigmaskHow` and signal set to the current mask. If `op` is `None`,
    /// this function does not change the mask.
    /// If `old_mask` is `Some`, this function sets the previous mask to it.
    fn sigmask(
        &mut self,
        op: Option<(SigmaskHow, &[signal::Number])>,
        old_mask: Option<&mut Vec<signal::Number>>,
    ) -> Result<()>;

    /// Gets and sets the handler for a signal.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::set_signal_handling`]. You should not call this function
    /// directly, or you will disrupt the behavior of `SharedSystem`. The
    /// description below applies if you want to do everything yourself without
    /// depending on `SharedSystem`.
    ///
    /// This is an abstract wrapper around the `sigaction` system call. This
    /// function returns the previous handler if successful.
    ///
    /// When you set the handler to `SignalHandling::Catch`, signals sent to
    /// this process are accumulated in the `System` instance and made available
    /// from [`caught_signals`](Self::caught_signals).
    fn sigaction(
        &mut self,
        signal: signal::Number,
        action: SignalHandling,
    ) -> Result<SignalHandling>;

    /// Returns signals this process has caught, if any.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::select`]. You should not call this function directly, or
    /// you will disrupt the behavior of `SharedSystem`. The description below
    /// applies if you want to do everything yourself without depending on
    /// `SharedSystem`.
    ///
    /// To catch a signal, you must set the signal handler to
    /// [`SignalHandling::Catch`] by calling [`sigaction`](Self::sigaction)
    /// first. Once the handler is ready, signals sent to the process are
    /// accumulated in the `System`. You call `caught_signals` to obtain a list
    /// of caught signals thus far.
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
    fn kill(
        &mut self,
        target: Pid,
        signal: Option<signal::Number>,
    ) -> Pin<Box<dyn Future<Output = Result<()>>>>;

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
        readers: &mut FdSet,
        writers: &mut FdSet,
        timeout: Option<&TimeSpec>,
        signal_mask: Option<&[signal::Number]>,
    ) -> Result<c_int>;

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
    fn tcsetpgrp(&mut self, fd: Fd, pgid: Pid) -> Result<()>;

    /// Creates a new child process.
    ///
    /// This is a thin wrapper around the `fork` system call. Users of `Env`
    /// should not call it directly. Instead, use [`Subshell`] so that the
    /// environment can condition the state of the child process before it
    /// starts running.
    ///
    /// If successful, this function returns a [`ChildProcessStarter`] function. The
    /// caller must call the starter exactly once to make sure the parent and
    /// child processes perform correctly after forking.
    fn new_child_process(&mut self) -> Result<ChildProcessStarter>;

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
    fn execve(&mut self, path: &CStr, args: &[CString], envs: &[CString]) -> Result<Infallible>;

    /// Returns the current working directory path.
    fn getcwd(&self) -> Result<PathBuf>;

    /// Changes the working directory.
    fn chdir(&mut self, path: &CStr) -> Result<()>;

    /// Returns the home directory path of the given user.
    ///
    /// Returns `Ok(None)` if the user is not found.
    fn getpwnam_dir(&self, name: &str) -> Result<Option<PathBuf>>;

    /// Returns the standard `$PATH` value where all standard utilities are
    /// expected to be found.
    ///
    /// This is a thin wrapper around the `confstr(_CS_PATH, …)`.
    fn confstr_path(&self) -> Result<OsString>;

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
    /// When no limit is set, the limit value is [`RLIM_INFINITY`].
    ///
    /// This is a thin wrapper around the `getrlimit` system call.
    ///
    /// [`RLIM_INFINITY`]: self::resource::RLIM_INFINITY
    fn getrlimit(&self, resource: Resource) -> std::io::Result<LimitPair>;

    /// Sets the limits for the specified resource.
    ///
    /// Specify [`RLIM_INFINITY`] as the limit value to remove the limit.
    ///
    /// This is a thin wrapper around the `setrlimit` system call.
    ///
    /// [`RLIM_INFINITY`]: self::resource::RLIM_INFINITY
    fn setrlimit(&mut self, resource: Resource, limits: LimitPair) -> std::io::Result<()>;
}

/// Sentinel for the current working directory
///
/// This value can be passed to system calls named "*at" such as
/// [`System::fstatat`].
pub const AT_FDCWD: Fd = Fd(nix::libc::AT_FDCWD);

/// Set of consumed CPU time
///
/// This structure contains four CPU time values, all in seconds.
///
/// This structure is returned by [`System::times`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Times {
    /// User CPU time consumed by the current process
    pub self_user: f64,
    /// System CPU time consumed by the current process
    pub self_system: f64,
    /// User CPU time consumed by the children of the current process
    pub children_user: f64,
    /// System CPU time consumed by the children of the current process
    pub children_system: f64,
}

/// How to handle a signal.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SignalHandling {
    /// Perform the default action for the signal.
    #[default]
    Default,
    /// Ignore the signal.
    Ignore,
    /// Catch the signal.
    Catch,
}

/// Task executed in a child process
///
/// This is an argument passed to a [`ChildProcessStarter`]. The task is
/// executed in a child process initiated by the starter. The environment passed
/// to the task is a clone of the parent environment, but it has a different
/// process ID than the parent.
pub type ChildProcessTask =
    Box<dyn for<'a> FnOnce(&'a mut Env) -> Pin<Box<dyn Future<Output = ()> + 'a>>>;

/// Abstract function that starts a child process
///
/// [`System::new_child_process`] returns a child process starter. You need to
/// pass the parent environment and a task to run in the child.
///
/// [`RealSystem`]'s `new_child_process` performs a `fork` system call and
/// returns a starter in the parent and child processes. When the starter is
/// called in the parent, it just returns the child process ID. The starter in
/// the child process runs the task and exits the process with the exit status
/// of the task.
///
/// For [`VirtualSystem`], no real child process is created. Instead, the
/// starter runs the task concurrently in the current process using the executor
/// contained in the system. A new [`Process`](virtual::Process) is added to the
/// system to represent the child process. The starter returns its process ID.
/// See also [`VirtualSystem::new_child_process`].
///
/// This function only starts the child, which continues to run asynchronously
/// after the function returns its PID. To wait for the child to finish and
/// obtain its exit status, use [`System::wait`].
pub type ChildProcessStarter = Box<
    dyn for<'a> FnOnce(&'a mut Env, ChildProcessTask) -> Pin<Box<dyn Future<Output = Pid> + 'a>>,
>;

/// Metadata of a file contained in a directory
///
/// `DirEntry` objects are enumerated by a [`Dir`] implementor.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct DirEntry<'a> {
    /// Filename
    pub name: &'a OsStr,
}

/// Trait for enumerating directory entries
///
/// An implementor of `Dir` may retain a file descriptor (or any other resource
/// alike) to access the underlying system and obtain entry information. The
/// file descriptor is released when the implementor object is dropped.
pub trait Dir: Debug {
    /// Returns the next directory entry.
    fn next(&mut self) -> Result<Option<DirEntry>>;
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
    /// function duplicates the file descriptor with [`System::dup`] and closes
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

        let new = self.dup(from, MIN_INTERNAL_FD, FdFlag::FD_CLOEXEC);
        self.close(from).ok();
        new
    }

    /// Tests if a file descriptor is a pipe.
    fn fd_is_pipe(&self, fd: Fd) -> bool {
        matches!(self.fstat(fd), Ok(stat)
            if SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT == SFlag::S_IFIFO)
    }

    /// Clears the `O_NONBLOCK` flag for the file descriptor.
    fn set_blocking(&mut self, fd: Fd) -> Result<()> {
        let flags = self.fcntl_getfl(fd)?;
        let new_flags = flags & !OFlag::O_NONBLOCK;
        if new_flags == flags {
            return Ok(());
        }
        self.fcntl_setfl(fd, new_flags)
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
    fn tcsetpgrp_with_block(&mut self, fd: Fd, pgid: Pid) -> Result<()> {
        let sigttou = self
            .signal_number_from_name(signal::Name::Ttou)
            .ok_or(Errno::EINVAL)?;
        let mut old_mask = Vec::new();

        self.sigmask(
            Some((SigmaskHow::SIG_BLOCK, &[sigttou])),
            Some(&mut old_mask),
        )?;

        let result = self.tcsetpgrp(fd, pgid);

        let result_2 = self.sigmask(Some((SigmaskHow::SIG_SETMASK, &old_mask)), None);

        result.or(result_2)
    }

    /// Switches the foreground process group with the default SIGTTOU settings.
    ///
    /// This is a convenience function to ensure the shell has been in the
    /// foreground and optionally change the foreground process group. This
    /// function calls [`sigaction`](System::sigaction) to restore the action
    /// for SIGTTOU to the default disposition (which is to suspend the shell
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
    fn tcsetpgrp_without_block(&mut self, fd: Fd, pgid: Pid) -> Result<()> {
        let sigttou = self
            .signal_number_from_name(signal::Name::Ttou)
            .ok_or(Errno::EINVAL)?;
        match self.sigaction(sigttou, SignalHandling::Default) {
            Err(e) => Err(e),
            Ok(old_handling) => {
                let mut old_mask = Vec::new();
                let result = match self.sigmask(
                    Some((SigmaskHow::SIG_UNBLOCK, &[sigttou])),
                    Some(&mut old_mask),
                ) {
                    Err(e) => Err(e),
                    Ok(()) => {
                        let result = self.tcsetpgrp(fd, pgid);

                        let result_2 =
                            self.sigmask(Some((SigmaskHow::SIG_SETMASK, &old_mask)), None);

                        result.or(result_2)
                    }
                };

                let result_2 = self.sigaction(sigttou, old_handling).map(drop);

                result.or(result_2)
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

    /// Returns the signal number that corresponds to the exit status.
    ///
    /// This function is basically the inverse of `impl From<signal::Number> for
    /// ExitStatus`. However, this function supports not only the offset of 384
    /// but also the offset of 128 and zero to accept exit statuses returned
    /// from other processes.
    #[must_use]
    fn signal_number_from_exit_status(&self, status: ExitStatus) -> Option<signal::Number> {
        [0x180, 0x80, 0].into_iter().find_map(|offset| {
            let raw_number = status.0.checked_sub(offset)?;
            self.validate_signal(raw_number).map(|(_, number)| number)
        })
    }
}

impl<T: System + ?Sized> SystemEx for T {}

/// System shared by a reference counter.
///
/// A `SharedSystem` is a reference-counted container of a [`System`] instance
/// accompanied with an internal state for supporting asynchronous interactions
/// with the system. As it is reference-counted, cloning a `SharedSystem`
/// instance only increments the reference count without cloning the backing
/// system instance. This behavior allows calling `SharedSystem`'s methods
/// concurrently from different `async` tasks that each have a `SharedSystem`
/// instance sharing the same state.
///
/// `SharedSystem` implements [`System`] by delegating to the contained system
/// instance. You should avoid calling some of the `System` methods, however.
/// Prefer `async` functions provided by `SharedSystem` (e.g.,
/// [`read_async`](Self::read_async)) over raw system functions (e.g.,
/// [`read`](System::read)).
///
/// The following example illustrates how multiple concurrent tasks are run in a
/// single-threaded pool:
///
/// ```
/// # use yash_env::{SharedSystem, System, VirtualSystem};
/// # use futures_util::task::LocalSpawnExt;
/// let mut system = SharedSystem::new(Box::new(VirtualSystem::new()));
/// let mut system2 = system.clone();
/// let mut system3 = system.clone();
/// let (reader, writer) = system.pipe().unwrap();
/// let mut executor = futures_executor::LocalPool::new();
///
/// // We add a task that tries to read from the pipe, but nothing has been
/// // written to it, so the task is stalled.
/// let read_task = executor.spawner().spawn_local_with_handle(async move {
///     let mut buffer = [0; 1];
///     system.read_async(reader, &mut buffer).await.unwrap();
///     buffer[0]
/// });
/// executor.run_until_stalled();
///
/// // Let's add a task that writes to the pipe.
/// executor.spawner().spawn_local(async move {
///     system2.write_all(writer, &[123]).await.unwrap();
/// });
/// executor.run_until_stalled();
///
/// // The write task has written a byte to the pipe, but the read task is still
/// // stalled. We need to wake it up by calling `select`.
/// system3.select(false).unwrap();
///
/// // Now the read task can proceed to the end.
/// let number = executor.run_until(read_task.unwrap());
/// assert_eq!(number, 123);
/// ```
///
/// If there is a child process in the [`VirtualSystem`], you should call
/// [`SystemState::select_all`](self::virtual::SystemState::select_all) in
/// addition to [`SharedSystem::select`] so that the child process task is woken
/// up when needed.
/// (TBD code example)
#[derive(Clone, Debug)]
pub struct SharedSystem(pub(crate) Rc<RefCell<SelectSystem>>);

impl SharedSystem {
    /// Creates a new shared system.
    pub fn new(system: Box<dyn System>) -> Self {
        SharedSystem(Rc::new(RefCell::new(SelectSystem::new(system))))
    }

    fn set_nonblocking(&mut self, fd: Fd) -> Result<OFlag> {
        let mut inner = self.0.borrow_mut();
        let flags = inner.fcntl_getfl(fd)?;
        if !flags.contains(OFlag::O_NONBLOCK) {
            inner.fcntl_setfl(fd, flags | OFlag::O_NONBLOCK)?;
        }
        Ok(flags)
    }

    fn reset_nonblocking(&mut self, fd: Fd, old_flags: OFlag) {
        if !old_flags.contains(OFlag::O_NONBLOCK) {
            let _: Result<()> = self.0.borrow_mut().fcntl_setfl(fd, old_flags);
        }
    }

    /// Reads from the file descriptor.
    ///
    /// This function waits for one or more bytes to be available for reading.
    /// If successful, returns the number of bytes read.
    pub async fn read_async(&mut self, fd: Fd, buffer: &mut [u8]) -> Result<usize> {
        let flags = self.set_nonblocking(fd)?;

        // We need to retain a strong reference to the waker outside the poll_fn
        // function because SelectSystem only retains a weak reference to it.
        // This allows SelectSystem to discard defunct wakers if this async task
        // is aborted.
        let waker = Rc::new(RefCell::new(None));

        let result = poll_fn(|context| {
            let mut inner = self.0.borrow_mut();
            match inner.read(fd, buffer) {
                Err(Errno::EAGAIN) => {
                    *waker.borrow_mut() = Some(context.waker().clone());
                    inner.add_reader(fd, Rc::downgrade(&waker));
                    Poll::Pending
                }
                result => Poll::Ready(result),
            }
        })
        .await;

        self.reset_nonblocking(fd, flags);

        result
    }

    /// Writes to the file descriptor.
    ///
    /// This function calls [`System::write`] repeatedly until the whole
    /// `buffer` is written to the FD. If the `buffer` is empty, `write` is not
    /// called at all, so any error that would be returned from `write` is not
    /// returned.
    ///
    /// This function silently ignores signals that may interrupt writes.
    pub async fn write_all(&mut self, fd: Fd, mut buffer: &[u8]) -> Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }

        let flags = self.set_nonblocking(fd)?;
        let mut written = 0;

        // We need to retain a strong reference to the waker outside the poll_fn
        // function because SelectSystem only retains a weak reference to it.
        // This allows SelectSystem to discard defunct wakers if this async task
        // is aborted.
        let waker = Rc::new(RefCell::new(None));

        let result = poll_fn(|context| {
            let mut inner = self.0.borrow_mut();
            match inner.write(fd, buffer) {
                Ok(count) => {
                    written += count;
                    buffer = &buffer[count..];
                    if buffer.is_empty() {
                        return Poll::Ready(Ok(written));
                    }
                }
                Err(Errno::EAGAIN | Errno::EINTR) => (),
                Err(error) => return Poll::Ready(Err(error)),
            }

            *waker.borrow_mut() = Some(context.waker().clone());
            inner.add_writer(fd, Rc::downgrade(&waker));
            Poll::Pending
        })
        .await;

        self.reset_nonblocking(fd, flags);

        result
    }

    /// Convenience function for printing a message to the standard error
    pub async fn print_error(&mut self, message: &str) {
        _ = self.write_all(Fd::STDERR, message.as_bytes()).await;
    }

    /// Waits until the specified time point.
    pub async fn wait_until(&self, target: Instant) {
        // We need to retain a strong reference to the waker outside the poll_fn
        // function because SelectSystem only retains a weak reference to it.
        // This allows SelectSystem to discard defunct wakers if this async task
        // is aborted.
        let waker = Rc::new(RefCell::new(None));

        poll_fn(|context| {
            let mut system = self.0.borrow_mut();
            let now = system.now();
            if now >= target {
                return Poll::Ready(());
            }
            *waker.borrow_mut() = Some(context.waker().clone());
            system.add_timeout(target, Rc::downgrade(&waker));
            Poll::Pending
        })
        .await
    }

    /// Waits for some signals to be delivered to this process.
    ///
    /// Before calling this function, you need to [set signal
    /// handling](Self::set_signal_handling) to `Catch`. Without doing so, this
    /// function cannot detect the receipt of the signals.
    ///
    /// Returns an array of signals that were caught.
    ///
    /// If this `SharedSystem` is part of an [`Env`], you should call
    /// [`Env::wait_for_signals`] rather than calling this function directly
    /// so that the trap set can remember the caught signal.
    pub async fn wait_for_signals(&self) -> Rc<[signal::Number]> {
        let status = self.0.borrow_mut().add_signal_waker();
        poll_fn(|context| {
            let mut status = status.borrow_mut();
            let dummy_status = SignalStatus::Expected(None);
            let old_status = std::mem::replace(&mut *status, dummy_status);
            match old_status {
                SignalStatus::Caught(signals) => Poll::Ready(signals),
                SignalStatus::Expected(_) => {
                    *status = SignalStatus::Expected(Some(context.waker().clone()));
                    Poll::Pending
                }
            }
        })
        .await
    }

    /// Waits for a signal to be delivered to this process.
    ///
    /// Before calling this function, you need to [set signal
    /// handling](Self::set_signal_handling) to `Catch`.
    /// Without doing so, this function cannot detect the receipt of the signal.
    ///
    /// If this `SharedSystem` is part of an [`Env`], you should call
    /// [`Env::wait_for_signal`] rather than calling this function directly
    /// so that the trap set can remember the caught signal.
    pub async fn wait_for_signal(&self, signal: signal::Number) {
        while !self.wait_for_signals().await.contains(&signal) {}
    }

    /// Waits for a next event to occur.
    ///
    /// This function calls [`System::select`] with arguments computed from the
    /// current internal state of the `SharedSystem`. It will wake up tasks
    /// waiting for the file descriptor to be ready in
    /// [`read_async`](Self::read_async) and [`write_all`](Self::write_all) or
    /// for a signal to be caught in [`wait_for_signal`](Self::wait_for_signal).
    /// If no tasks are woken for FDs or signals and `poll` is false, this
    /// function will block until the first task waiting for a specific time
    /// point is woken.
    ///
    /// If poll is true, this function does not block, so it may not wake up any
    /// tasks.
    ///
    /// This function may wake up a task even if the condition it is expecting
    /// has not yet been met.
    pub fn select(&self, poll: bool) -> Result<()> {
        self.0.borrow_mut().select(poll)
    }
}

impl System for SharedSystem {
    fn fstat(&self, fd: Fd) -> Result<FileStat> {
        self.0.borrow().fstat(fd)
    }
    fn fstatat(&self, dir_fd: Fd, path: &CStr, flags: AtFlags) -> Result<FileStat> {
        self.0.borrow().fstatat(dir_fd, path, flags)
    }
    fn is_executable_file(&self, path: &CStr) -> bool {
        self.0.borrow().is_executable_file(path)
    }
    fn is_directory(&self, path: &CStr) -> bool {
        self.0.borrow().is_directory(path)
    }
    fn pipe(&mut self) -> Result<(Fd, Fd)> {
        self.0.borrow_mut().pipe()
    }
    fn dup(&mut self, from: Fd, to_min: Fd, flags: FdFlag) -> Result<Fd> {
        self.0.borrow_mut().dup(from, to_min, flags)
    }
    fn dup2(&mut self, from: Fd, to: Fd) -> Result<Fd> {
        self.0.borrow_mut().dup2(from, to)
    }
    fn open(&mut self, path: &CStr, option: OFlag, mode: Mode) -> Result<Fd> {
        self.0.borrow_mut().open(path, option, mode)
    }
    fn open_tmpfile(&mut self, parent_dir: &Path) -> Result<Fd> {
        self.0.borrow_mut().open_tmpfile(parent_dir)
    }
    fn close(&mut self, fd: Fd) -> Result<()> {
        self.0.borrow_mut().close(fd)
    }
    fn fcntl_getfl(&self, fd: Fd) -> Result<OFlag> {
        self.0.borrow().fcntl_getfl(fd)
    }
    fn fcntl_setfl(&mut self, fd: Fd, flags: OFlag) -> Result<()> {
        self.0.borrow_mut().fcntl_setfl(fd, flags)
    }
    fn fcntl_getfd(&self, fd: Fd) -> Result<FdFlag> {
        self.0.borrow().fcntl_getfd(fd)
    }
    fn fcntl_setfd(&mut self, fd: Fd, flags: FdFlag) -> Result<()> {
        self.0.borrow_mut().fcntl_setfd(fd, flags)
    }
    fn isatty(&self, fd: Fd) -> Result<bool> {
        self.0.borrow().isatty(fd)
    }
    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> Result<usize> {
        self.0.borrow_mut().read(fd, buffer)
    }
    fn write(&mut self, fd: Fd, buffer: &[u8]) -> Result<usize> {
        self.0.borrow_mut().write(fd, buffer)
    }
    fn lseek(&mut self, fd: Fd, position: SeekFrom) -> Result<u64> {
        self.0.borrow_mut().lseek(fd, position)
    }
    fn fdopendir(&mut self, fd: Fd) -> Result<Box<dyn Dir>> {
        self.0.borrow_mut().fdopendir(fd)
    }
    fn opendir(&mut self, path: &CStr) -> Result<Box<dyn Dir>> {
        self.0.borrow_mut().opendir(path)
    }
    fn umask(&mut self, mask: Mode) -> Mode {
        self.0.borrow_mut().umask(mask)
    }
    fn now(&self) -> Instant {
        self.0.borrow().now()
    }
    fn times(&self) -> Result<Times> {
        self.0.borrow().times()
    }
    fn validate_signal(&self, number: signal::RawNumber) -> Option<(signal::Name, signal::Number)> {
        self.0.borrow().validate_signal(number)
    }
    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number> {
        self.0.borrow().signal_number_from_name(name)
    }
    fn sigmask(
        &mut self,
        op: Option<(SigmaskHow, &[signal::Number])>,
        old_mask: Option<&mut Vec<signal::Number>>,
    ) -> Result<()> {
        (**self.0.borrow_mut()).sigmask(op, old_mask)
    }
    fn sigaction(
        &mut self,
        signal: signal::Number,
        action: SignalHandling,
    ) -> Result<SignalHandling> {
        self.0.borrow_mut().sigaction(signal, action)
    }
    fn caught_signals(&mut self) -> Vec<signal::Number> {
        self.0.borrow_mut().caught_signals()
    }
    fn kill(
        &mut self,
        target: Pid,
        signal: Option<signal::Number>,
    ) -> Pin<Box<(dyn Future<Output = Result<()>>)>> {
        self.0.borrow_mut().kill(target, signal)
    }
    fn select(
        &mut self,
        readers: &mut FdSet,
        writers: &mut FdSet,
        timeout: Option<&TimeSpec>,
        signal_mask: Option<&[signal::Number]>,
    ) -> Result<c_int> {
        (**self.0.borrow_mut()).select(readers, writers, timeout, signal_mask)
    }
    fn getpid(&self) -> Pid {
        self.0.borrow().getpid()
    }
    fn getppid(&self) -> Pid {
        self.0.borrow().getppid()
    }
    fn getpgrp(&self) -> Pid {
        self.0.borrow().getpgrp()
    }
    fn setpgid(&mut self, pid: Pid, pgid: Pid) -> Result<()> {
        self.0.borrow_mut().setpgid(pid, pgid)
    }
    fn tcgetpgrp(&self, fd: Fd) -> Result<Pid> {
        self.0.borrow().tcgetpgrp(fd)
    }
    fn tcsetpgrp(&mut self, fd: Fd, pgid: Pid) -> Result<()> {
        self.0.borrow_mut().tcsetpgrp(fd, pgid)
    }
    fn new_child_process(&mut self) -> Result<ChildProcessStarter> {
        self.0.borrow_mut().new_child_process()
    }
    fn wait(&mut self, target: Pid) -> Result<Option<(Pid, ProcessState)>> {
        self.0.borrow_mut().wait(target)
    }
    fn execve(&mut self, path: &CStr, args: &[CString], envs: &[CString]) -> Result<Infallible> {
        self.0.borrow_mut().execve(path, args, envs)
    }
    fn getcwd(&self) -> Result<PathBuf> {
        self.0.borrow().getcwd()
    }
    fn chdir(&mut self, path: &CStr) -> Result<()> {
        self.0.borrow_mut().chdir(path)
    }
    fn getpwnam_dir(&self, name: &str) -> Result<Option<PathBuf>> {
        self.0.borrow().getpwnam_dir(name)
    }
    fn confstr_path(&self) -> Result<OsString> {
        self.0.borrow().confstr_path()
    }
    fn shell_path(&self) -> CString {
        self.0.borrow().shell_path()
    }
    fn getrlimit(&self, resource: Resource) -> std::io::Result<LimitPair> {
        self.0.borrow().getrlimit(resource)
    }
    fn setrlimit(&mut self, resource: Resource, limits: LimitPair) -> std::io::Result<()> {
        self.0.borrow_mut().setrlimit(resource, limits)
    }
}

impl SignalSystem for SharedSystem {
    #[inline]
    fn signal_name_from_number(&self, number: signal::Number) -> signal::Name {
        SystemEx::signal_name_from_number(self, number)
    }

    #[inline]
    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number> {
        System::signal_number_from_name(self, name)
    }

    fn set_signal_handling(
        &mut self,
        signal: signal::Number,
        handling: SignalHandling,
    ) -> Result<SignalHandling> {
        self.0.borrow_mut().set_signal_handling(signal, handling)
    }
}

#[cfg(test)]
mod tests {
    use super::r#virtual::VirtualSystem;
    use super::r#virtual::PIPE_SIZE;
    use super::r#virtual::{SIGCHLD, SIGINT, SIGTERM, SIGUSR1};
    use super::*;
    use assert_matches::assert_matches;
    use futures_util::task::noop_waker_ref;
    use futures_util::FutureExt;
    use std::future::Future;
    use std::rc::Rc;
    use std::task::Context;
    use std::time::Duration;

    #[test]
    fn shared_system_read_async_ready() {
        let mut system = SharedSystem::new(Box::new(VirtualSystem::new()));
        let (reader, writer) = system.pipe().unwrap();
        system.write(writer, &[42]).unwrap();

        let mut buffer = [0; 2];
        let result = system.read_async(reader, &mut buffer).now_or_never();
        assert_eq!(result, Some(Ok(1)));
        assert_eq!(buffer[..1], [42]);
    }

    #[test]
    fn shared_system_read_async_not_ready_at_first() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        let system2 = system.clone();
        let (reader, writer) = system.pipe().unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut buffer = [0; 2];
        let mut future = Box::pin(system.read_async(reader, &mut buffer));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        let result = system2.select(false);
        assert_eq!(result, Ok(()));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        state.borrow_mut().processes[&process_id].fds[&writer]
            .open_file_description
            .borrow_mut()
            .write(&[56])
            .unwrap();

        let result = future.as_mut().poll(&mut context);
        drop(future);
        assert_eq!(result, Poll::Ready(Ok(1)));
        assert_eq!(buffer[..1], [56]);
    }

    #[test]
    fn shared_system_write_all_ready() {
        let mut system = SharedSystem::new(Box::new(VirtualSystem::new()));
        let (reader, writer) = system.pipe().unwrap();
        let result = system.write_all(writer, &[17]).now_or_never().unwrap();
        assert_eq!(result, Ok(1));

        let mut buffer = [0; 2];
        system.read(reader, &mut buffer).unwrap();
        assert_eq!(buffer[..1], [17]);
    }

    #[test]
    fn shared_system_write_all_not_ready_at_first() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        let (reader, writer) = system.pipe().unwrap();

        state.borrow_mut().processes[&process_id].fds[&writer]
            .open_file_description
            .borrow_mut()
            .write(&[42; PIPE_SIZE])
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut out_buffer = [87; PIPE_SIZE];
        out_buffer[0] = 0;
        out_buffer[1] = 1;
        out_buffer[PIPE_SIZE - 2] = 0xFE;
        out_buffer[PIPE_SIZE - 1] = 0xFF;
        let mut future = Box::pin(system.write_all(writer, &out_buffer));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        let mut in_buffer = [0; PIPE_SIZE - 1];
        state.borrow_mut().processes[&process_id].fds[&reader]
            .open_file_description
            .borrow_mut()
            .read(&mut in_buffer)
            .unwrap();
        assert_eq!(in_buffer, [42; PIPE_SIZE - 1]);

        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        in_buffer[0] = 0;
        state.borrow_mut().processes[&process_id].fds[&reader]
            .open_file_description
            .borrow_mut()
            .read(&mut in_buffer[..1])
            .unwrap();
        assert_eq!(in_buffer[..1], [42; 1]);

        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Ready(Ok(out_buffer.len())));

        state.borrow_mut().processes[&process_id].fds[&reader]
            .open_file_description
            .borrow_mut()
            .read(&mut in_buffer)
            .unwrap();
        assert_eq!(in_buffer, out_buffer[..PIPE_SIZE - 1]);
        state.borrow_mut().processes[&process_id].fds[&reader]
            .open_file_description
            .borrow_mut()
            .read(&mut in_buffer)
            .unwrap();
        assert_eq!(in_buffer[..1], out_buffer[PIPE_SIZE - 1..]);
    }

    #[test]
    fn shared_system_write_all_empty() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        let (_reader, writer) = system.pipe().unwrap();

        state.borrow_mut().processes[&process_id].fds[&writer]
            .open_file_description
            .borrow_mut()
            .write(&[0; PIPE_SIZE])
            .unwrap();

        // Even if the pipe is full, empty write succeeds.
        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.write_all(writer, &[]));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Ready(Ok(0)));
        // TODO Make sure `write` is not called at all
    }

    // TODO Test SharedSystem::write_all where second write returns EINTR

    #[test]
    fn shared_system_wait_until() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let system = SharedSystem::new(Box::new(system));
        let start = Instant::now();
        state.borrow_mut().now = Some(start);
        let target = start + Duration::from_millis(1_125);

        let mut future = Box::pin(system.wait_until(target));
        let mut context = Context::from_waker(noop_waker_ref());
        let poll = future.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);

        system.select(false).unwrap();
        let poll = future.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Ready(()));
        assert_eq!(state.borrow().now, Some(target));
    }

    #[test]
    fn shared_system_wait_for_signals() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        system
            .set_signal_handling(SIGCHLD, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(SIGINT, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(SIGUSR1, SignalHandling::Catch)
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.wait_for_signals());
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            assert!(process.blocked_signals().contains(&SIGCHLD));
            assert!(process.blocked_signals().contains(&SIGINT));
            assert!(process.blocked_signals().contains(&SIGUSR1));
            let _ = process.raise_signal(SIGCHLD);
            let _ = process.raise_signal(SIGINT);
        }
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        system.select(false).unwrap();
        let result = future.as_mut().poll(&mut context);
        assert_matches!(result, Poll::Ready(signals) => {
            assert_eq!(signals.len(), 2);
            assert!(signals.contains(&SIGCHLD));
            assert!(signals.contains(&SIGINT));
        });
    }

    #[test]
    fn shared_system_wait_for_signal_returns_on_caught() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        system
            .set_signal_handling(SIGCHLD, SignalHandling::Catch)
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.wait_for_signal(SIGCHLD));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            assert!(process.blocked_signals().contains(&SIGCHLD));
            let _ = process.raise_signal(SIGCHLD);
        }
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        system.select(false).unwrap();
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Ready(()));
    }

    #[test]
    fn shared_system_wait_for_signal_ignores_irrelevant_signals() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        system
            .set_signal_handling(SIGINT, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(SIGTERM, SignalHandling::Catch)
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.wait_for_signal(SIGINT));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            let _ = process.raise_signal(SIGCHLD);
            let _ = process.raise_signal(SIGTERM);
        }
        system.select(false).unwrap();

        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);
    }

    #[test]
    fn shared_system_select_consumes_all_pending_signals() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        system
            .set_signal_handling(SIGINT, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(SIGTERM, SignalHandling::Catch)
            .unwrap();

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            let _ = process.raise_signal(SIGINT);
            let _ = process.raise_signal(SIGTERM);
        }
        system.select(false).unwrap();

        let state = state.borrow();
        let process = state.processes.get(&process_id).unwrap();
        let blocked = process.blocked_signals();
        assert!(blocked.contains(&SIGINT));
        assert!(blocked.contains(&SIGTERM));
        let pending = process.pending_signals();
        assert!(!pending.contains(&SIGINT));
        assert!(!pending.contains(&SIGTERM));
    }

    #[test]
    fn shared_system_select_does_not_wake_signal_waiters_on_io() {
        let system = VirtualSystem::new();
        let mut system_1 = SharedSystem::new(Box::new(system));
        let mut system_2 = system_1.clone();
        let mut system_3 = system_1.clone();
        let (reader, writer) = system_1.pipe().unwrap();
        system_2
            .set_signal_handling(SIGCHLD, SignalHandling::Catch)
            .unwrap();

        let mut buffer = [0];
        let mut read_future = Box::pin(system_1.read_async(reader, &mut buffer));
        let mut signal_future = Box::pin(system_2.wait_for_signals());
        let mut context = Context::from_waker(noop_waker_ref());
        let result = read_future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);
        let result = signal_future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);
        system_3.write(writer, &[42]).unwrap();
        system_3.select(false).unwrap();

        let result = read_future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Ready(Ok(1)));
        let result = signal_future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);
    }

    #[test]
    fn shared_system_select_poll() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let system = SharedSystem::new(Box::new(system));
        let start = Instant::now();
        state.borrow_mut().now = Some(start);
        let target = start + Duration::from_millis(1_125);

        let mut future = Box::pin(system.wait_until(target));
        let mut context = Context::from_waker(noop_waker_ref());
        let poll = future.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);

        system.select(true).unwrap();
        let poll = future.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert_eq!(state.borrow().now, Some(start));
    }
}
