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
use crate::io::Fd;
use crate::io::MIN_INTERNAL_FD;
use crate::job::Pid;
use crate::job::ProcessState;
use crate::semantics::ExitStatus;
#[cfg(doc)]
use crate::subshell::Subshell;
use crate::trap::Signal;
use crate::trap::Signal2;
use crate::trap::SignalSystem;
use crate::trap::UnknownSignalError;
use crate::Env;
use futures_util::future::poll_fn;
use futures_util::task::Poll;
#[doc(no_inline)]
pub use nix::fcntl::AtFlags;
#[doc(no_inline)]
pub use nix::fcntl::FdFlag;
#[doc(no_inline)]
pub use nix::fcntl::OFlag;
// TODO Remove this
#[doc(no_inline)]
pub use nix::sys::signal::SigSet;
#[doc(no_inline)]
pub use nix::sys::signal::SigmaskHow;
#[doc(no_inline)]
pub use nix::sys::stat::{FileStat, Mode, SFlag};
#[doc(no_inline)]
pub use nix::sys::time::TimeSpec;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::cmp::Reverse;
use std::collections::binary_heap::PeekMut;
use std::collections::BinaryHeap;
use std::convert::Infallible;
use std::ffi::c_int;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fmt::Debug;
use std::future::Future;
use std::io::SeekFrom;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::rc::Weak;
use std::task::Waker;
use std::time::Duration;
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

    /// Returns the raw signal number for the given signal.
    ///
    /// If the signal is not supported by the system, this function returns
    /// `None`.
    fn signal_to_raw_number(
        &self,
        signal: Signal2,
    ) -> std::result::Result<c_int, UnknownSignalError>;

    /// Returns the signal for the given raw signal number.
    ///
    /// If the number does not correspond to any signal, this function returns
    /// `None`.
    fn raw_number_to_signal(
        &self,
        number: c_int,
    ) -> std::result::Result<Signal2, UnknownSignalError>;

    /// Gets and/or sets the signal blocking mask.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::set_signal_handling`]. You should not call this function
    /// directly, or you will disrupt the behavior of `SharedSystem`. The
    /// description below applies if you want to do everything yourself without
    /// depending on `SharedSystem`.
    ///
    /// This is a thin wrapper around the `sigprocmask` system call. If `set` is
    /// `Some`, this function updates the signal blocking mask according to
    /// `how`. If `oldset` is `Some`, this function sets the previous mask to
    /// it.
    fn sigmask(
        &mut self,
        how: SigmaskHow,
        set: Option<&[Signal]>,
        old_set: Option<&mut Vec<Signal>>,
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
    fn sigaction(&mut self, signal: Signal, action: SignalHandling) -> Result<SignalHandling>;

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
    fn caught_signals(&mut self) -> Vec<Signal>;

    /// Sends a signal.
    ///
    /// This is a thin wrapper around the `kill` system call.
    ///
    /// The virtual system version of this function blocks the calling thread if
    /// the signal stops or terminates the current process, hence returning a
    /// future. See [`VirtualSystem::kill`] for details.
    fn kill(
        &mut self,
        target: Pid,
        signal: Option<Signal>,
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
    /// If `signal_mask` is `Some` signal set, the signal blocking mask is set
    /// to it while waiting and restored when the function returns.
    fn select(
        &mut self,
        readers: &mut FdSet,
        writers: &mut FdSet,
        timeout: Option<&TimeSpec>,
        signal_mask: Option<&[Signal]>,
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

    /// Normalizes a signal.
    ///
    /// Different signal names may refer to the same signal number. This
    /// function normalizes the signal to a canonical name.
    fn normalize_signal(
        &self,
        signal: Signal2,
    ) -> std::result::Result<Signal2, UnknownSignalError> {
        let number = self.signal_to_raw_number(signal)?;
        self.raw_number_to_signal(number)
    }

    /// Returns the exit status for a process that was terminated by the
    /// specified signal.
    ///
    /// If the signal is not supported by the system, this function returns
    /// `ExitStatus(i32::MAX)`
    #[must_use]
    fn exit_status_for_signal(&self, signal: Signal2) -> ExitStatus {
        let raw_exit_status = match self.signal_to_raw_number(signal) {
            // POSIX requires the offset to be 0x80 or greater. We additionally
            // add 0x100 to make sure the exit status is distinguishable from those
            // of processes that exited normally.
            Ok(number) => number.saturating_add(0x180),
            Err(UnknownSignalError) => i32::MAX,
        };
        ExitStatus(raw_exit_status)
    }

    /// Returns the exit status for a process that has the specified state.
    ///
    /// This function returns `None` if the state is `Running`.
    #[must_use]
    fn exit_status_for_process_state(&self, state: ProcessState) -> Option<ExitStatus> {
        match state {
            ProcessState::Exited(exit_status) => Some(exit_status),
            ProcessState::Signaled { signal, .. } | ProcessState::Stopped(signal) => {
                Some(self.exit_status_for_signal(self.raw_number_to_signal(signal as _).unwrap()))
            }
            ProcessState::Running => None,
        }
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
        let mut old_set = Vec::new();
        self.sigmask(
            SigmaskHow::SIG_BLOCK,
            Some(&[Signal::SIGTTOU]),
            Some(&mut old_set),
        )?;

        let result = self.tcsetpgrp(fd, pgid);

        let result_2 = self.sigmask(SigmaskHow::SIG_SETMASK, Some(&old_set), None);

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
        match self.sigaction(Signal::SIGTTOU, SignalHandling::Default) {
            Err(e) => Err(e),
            Ok(old_handling) => {
                let mut old_set = Vec::new();
                let result = match self.sigmask(
                    SigmaskHow::SIG_UNBLOCK,
                    Some(&[Signal::SIGTTOU]),
                    Some(&mut old_set),
                ) {
                    Err(e) => Err(e),
                    Ok(()) => {
                        let result = self.tcsetpgrp(fd, pgid);

                        let result_2 = self.sigmask(SigmaskHow::SIG_SETMASK, Some(&old_set), None);

                        result.or(result_2)
                    }
                };

                let result_2 = self.sigaction(Signal::SIGTTOU, old_handling).map(drop);

                result.or(result_2)
            }
        }
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
        let flags = inner.system.fcntl_getfl(fd)?;
        if !flags.contains(OFlag::O_NONBLOCK) {
            inner.system.fcntl_setfl(fd, flags | OFlag::O_NONBLOCK)?;
        }
        Ok(flags)
    }

    fn reset_nonblocking(&mut self, fd: Fd, old_flags: OFlag) {
        if !old_flags.contains(OFlag::O_NONBLOCK) {
            let _: Result<()> = self.0.borrow_mut().system.fcntl_setfl(fd, old_flags);
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
            match inner.system.read(fd, buffer) {
                Err(Errno::EAGAIN) => {
                    *waker.borrow_mut() = Some(context.waker().clone());
                    inner.io.wait_for_reading(fd, &waker);
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
            match inner.system.write(fd, buffer) {
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
            inner.io.wait_for_writing(fd, &waker);
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
            let waker = Rc::downgrade(&waker);
            system.time.push(Timeout { target, waker });
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
    pub async fn wait_for_signals(&self) -> Rc<[Signal]> {
        let status = self.0.borrow_mut().signal.wait_for_signals();
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
    pub async fn wait_for_signal(&self, signal: Signal) {
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
    fn signal_to_raw_number(
        &self,
        signal: Signal2,
    ) -> std::result::Result<c_int, UnknownSignalError> {
        self.0.borrow().signal_to_raw_number(signal)
    }
    fn raw_number_to_signal(
        &self,
        number: c_int,
    ) -> std::result::Result<Signal2, UnknownSignalError> {
        self.0.borrow().raw_number_to_signal(number)
    }
    fn sigmask(
        &mut self,
        how: SigmaskHow,
        set: Option<&[Signal]>,
        old_set: Option<&mut Vec<Signal>>,
    ) -> Result<()> {
        (**self.0.borrow_mut()).sigmask(how, set, old_set)
    }
    fn sigaction(&mut self, signal: Signal, action: SignalHandling) -> Result<SignalHandling> {
        self.0.borrow_mut().sigaction(signal, action)
    }
    fn caught_signals(&mut self) -> Vec<Signal> {
        self.0.borrow_mut().caught_signals()
    }
    fn kill(
        &mut self,
        target: Pid,
        signal: Option<Signal>,
    ) -> Pin<Box<(dyn Future<Output = Result<()>>)>> {
        self.0.borrow_mut().kill(target, signal)
    }
    fn select(
        &mut self,
        readers: &mut FdSet,
        writers: &mut FdSet,
        timeout: Option<&TimeSpec>,
        signal_mask: Option<&[Signal]>,
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
    fn set_signal_handling(
        &mut self,
        signal: nix::sys::signal::Signal,
        handling: SignalHandling,
    ) -> Result<SignalHandling> {
        self.0.borrow_mut().set_signal_handling(signal, handling)
    }
}

/// [System] extended with internal state to support asynchronous functions.
///
/// `SelectSystem` wraps a `System` instance and manages the internal state for
/// asynchronous I/O, signal handling, and timer functions. It coordinates
/// wakers for asynchronous I/O, signals, and timers to call `select` with the
/// appropriate arguments and wake up the wakers when the corresponding events
/// occur.
#[derive(Debug)]
pub(crate) struct SelectSystem {
    /// System instance that performs actual system calls
    system: Box<dyn System>,
    /// Helper for `select`ing on file descriptors
    io: AsyncIo,
    /// Helper for `select`ing on time
    time: AsyncTime,
    /// Helper for `select`ing on signals
    signal: AsyncSignal,
    /// Set of signals passed to `select`
    ///
    /// This is the mask the shell inherited from the parent shell minus the
    /// signals the shell wants to catch.
    wait_mask: Option<Vec<Signal>>,
}

impl Deref for SelectSystem {
    type Target = Box<dyn System>;
    fn deref(&self) -> &Box<dyn System> {
        &self.system
    }
}

impl DerefMut for SelectSystem {
    fn deref_mut(&mut self) -> &mut Box<dyn System> {
        &mut self.system
    }
}

impl SelectSystem {
    /// Creates a new `SelectSystem` that wraps the given `System`.
    pub fn new(system: Box<dyn System>) -> Self {
        SelectSystem {
            system,
            io: AsyncIo::new(),
            time: AsyncTime::new(),
            signal: AsyncSignal::new(),
            wait_mask: None,
        }
    }

    /// Calls `sigmask` and updates `self.wait_mask`.
    fn sigmask(&mut self, how: SigmaskHow, signal: Signal) -> Result<()> {
        match &mut self.wait_mask {
            None => {
                let mut old_set = Vec::new();
                self.system
                    .sigmask(how, Some(&[signal]), Some(&mut old_set))?;
                old_set.retain(|&s| s != signal);
                self.wait_mask = Some(old_set);
            }
            Some(wait_mask) => {
                self.system.sigmask(how, Some(&[signal]), None)?;
                wait_mask.retain(|&s| s != signal);
            }
        }
        Ok(())
    }

    /// Implements signal handler update.
    ///
    /// See [`SharedSystem::set_signal_handling`].
    pub fn set_signal_handling(
        &mut self,
        signal: Signal,
        handling: SignalHandling,
    ) -> Result<SignalHandling> {
        // The order of sigmask and sigaction is important to prevent the signal
        // from being caught. The signal must be caught only when the select
        // function temporarily unblocks the signal. This is to avoid race
        // condition.
        match handling {
            SignalHandling::Default | SignalHandling::Ignore => {
                let old_handling = self.system.sigaction(signal, handling)?;
                self.sigmask(SigmaskHow::SIG_UNBLOCK, signal)?;
                Ok(old_handling)
            }
            SignalHandling::Catch => {
                self.sigmask(SigmaskHow::SIG_BLOCK, signal)?;
                self.system.sigaction(signal, handling)
            }
        }
    }

    fn wake_timeouts(&mut self) {
        if !self.time.is_empty() {
            let now = self.now();
            self.time.wake_if_passed(now);
        }
        self.time.gc();
    }

    fn wake_on_signals(&mut self) {
        let signals = self.system.caught_signals();
        if signals.is_empty() {
            self.signal.gc()
        } else {
            self.signal.wake(&signals.into())
        }
    }

    /// Implements the select function for `SharedSystem`.
    ///
    /// See [`SharedSystem::select`].
    pub fn select(&mut self, poll: bool) -> Result<()> {
        let mut readers = self.io.readers();
        let mut writers = self.io.writers();
        let timeout = if poll {
            Some(TimeSpec::from(Duration::ZERO))
        } else {
            self.time.first_target().map(|t| {
                let now = self.now();
                let duration = t.saturating_duration_since(now);
                TimeSpec::from(duration)
            })
        };

        let inner_result = self.system.select(
            &mut readers,
            &mut writers,
            timeout.as_ref(),
            self.wait_mask.as_deref(),
        );
        let final_result = match inner_result {
            Ok(_) => {
                self.io.wake(readers, writers);
                Ok(())
            }
            Err(Errno::EBADF) => {
                // Some of the readers and writers are invalid but we cannot
                // tell which, so we wake up everything.
                self.io.wake_all();
                Err(Errno::EBADF)
            }
            Err(Errno::EINTR) => Ok(()),
            Err(error) => Err(error),
        };
        self.io.gc();
        self.wake_timeouts();
        self.wake_on_signals();
        final_result
    }
}

/// Helper for `select`ing on file descriptors
///
/// An `AsyncIo` is a set of [`Waker`]s that are waiting for an FD to be ready for
/// reading or writing. It computes the set of FDs to pass to the `select` system
/// call and wakes the corresponding wakers when the FDs are ready.
#[derive(Clone, Debug, Default)]
struct AsyncIo {
    readers: Vec<FdAwaiter>,
    writers: Vec<FdAwaiter>,
}

#[derive(Clone, Debug)]
struct FdAwaiter {
    fd: Fd,
    waker: Weak<RefCell<Option<Waker>>>,
}

/// Wakes the waker when `FdAwaiter` is dropped.
impl Drop for FdAwaiter {
    fn drop(&mut self) {
        if let Some(waker) = self.waker.upgrade() {
            if let Some(waker) = waker.borrow_mut().take() {
                waker.wake();
            }
        }
    }
}

impl AsyncIo {
    /// Returns a new empty `AsyncIo`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a set of FDs waiting for reading.
    ///
    /// The return value should be passed to the `select` or `pselect` system
    /// call.
    pub fn readers(&self) -> FdSet {
        let mut set = FdSet::new();
        for reader in &self.readers {
            set.insert(reader.fd)
                .expect("file descriptor out of supported range");
        }
        set
    }

    /// Returns a set of FDs waiting for writing.
    ///
    /// The return value should be passed to the `select` or `pselect` system
    /// call.
    pub fn writers(&self) -> FdSet {
        let mut set = FdSet::new();
        for writer in &self.writers {
            set.insert(writer.fd)
                .expect("file descriptor out of supported range");
        }
        set
    }

    /// Adds an awaiter for reading.
    pub fn wait_for_reading(&mut self, fd: Fd, waker: &Rc<RefCell<Option<Waker>>>) {
        let waker = Rc::downgrade(waker);
        self.readers.push(FdAwaiter { fd, waker });
    }

    /// Adds an awaiter for writing.
    pub fn wait_for_writing(&mut self, fd: Fd, waker: &Rc<RefCell<Option<Waker>>>) {
        let waker = Rc::downgrade(waker);
        self.writers.push(FdAwaiter { fd, waker });
    }

    /// Wakes awaiters that are ready for reading/writing.
    ///
    /// FDs in `readers` and `writers` are considered ready and corresponding
    /// awaiters are woken. Once woken, awaiters are removed from `self`.
    pub fn wake(&mut self, readers: FdSet, writers: FdSet) {
        self.readers.retain(|awaiter| !readers.contains(awaiter.fd));
        self.writers.retain(|awaiter| !writers.contains(awaiter.fd));
    }

    /// Wakes and removes all awaiters.
    pub fn wake_all(&mut self) {
        self.readers.clear();
        self.writers.clear();
    }

    /// Discards `FdAwaiter`s having a defunct waker.
    pub fn gc(&mut self) {
        let is_alive = |awaiter: &FdAwaiter| awaiter.waker.strong_count() > 0;
        self.readers.retain(is_alive);
        self.writers.retain(is_alive);
    }
}

/// Helper for `select`ing on time
///
/// An `AsyncTime` is a set of [`Waker`]s that are waiting for a specific time
/// to come. It wakes the wakers when the time is reached.
#[derive(Clone, Debug, Default)]
struct AsyncTime {
    timeouts: BinaryHeap<Reverse<Timeout>>,
}

#[derive(Clone, Debug)]
struct Timeout {
    target: Instant,
    waker: Weak<RefCell<Option<Waker>>>,
}

impl PartialEq for Timeout {
    fn eq(&self, rhs: &Self) -> bool {
        self.target == rhs.target
    }
}

impl Eq for Timeout {}

impl PartialOrd for Timeout {
    fn partial_cmp(&self, rhs: &Self) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}

impl Ord for Timeout {
    fn cmp(&self, rhs: &Self) -> Ordering {
        self.target.cmp(&rhs.target)
    }
}

/// Wakes the waker when `Timeout` is dropped.
impl Drop for Timeout {
    fn drop(&mut self) {
        if let Some(waker) = self.waker.upgrade() {
            if let Some(waker) = waker.borrow_mut().take() {
                waker.wake();
            }
        }
    }
}

impl AsyncTime {
    #[must_use]
    fn new() -> Self {
        Self::default()
    }

    #[must_use]
    fn is_empty(&self) -> bool {
        self.timeouts.is_empty()
    }

    fn push(&mut self, timeout: Timeout) {
        self.timeouts.push(Reverse(timeout))
    }

    #[must_use]
    fn first_target(&self) -> Option<Instant> {
        self.timeouts.peek().map(|timeout| timeout.0.target)
    }

    fn wake_if_passed(&mut self, now: Instant) {
        while let Some(timeout) = self.timeouts.peek_mut() {
            if !timeout.0.passed(now) {
                break;
            }
            PeekMut::pop(timeout);
        }
    }

    fn gc(&mut self) {
        self.timeouts.retain(|t| t.0.waker.strong_count() > 0);
    }
}

impl Timeout {
    fn passed(&self, now: Instant) -> bool {
        self.target <= now
    }
}

/// Helper for `select`ing on signals
///
/// An `AsyncSignal` is a set of [`Waker`]s that are waiting for a signal to be
/// caught by the current process. It wakes the wakers when signals are caught.
#[derive(Clone, Debug, Default)]
struct AsyncSignal {
    awaiters: Vec<Weak<RefCell<SignalStatus>>>,
}

#[derive(Clone, Debug)]
enum SignalStatus {
    Expected(Option<Waker>),
    Caught(Rc<[Signal]>),
}

impl AsyncSignal {
    /// Returns a new empty `AsyncSignal`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes internal weak pointers whose `SignalStatus` has gone.
    pub fn gc(&mut self) {
        self.awaiters.retain(|awaiter| awaiter.strong_count() > 0);
    }

    /// Adds an awaiter for signals.
    ///
    /// This function returns a reference-counted
    /// `SignalStatus::Expected(None)`. The caller must set a waker to the
    /// returned `SignalStatus::Expected`. When signals are caught, the waker is
    /// woken and replaced with `SignalStatus::Caught(signals)`. The caller can
    /// replace the waker in the `SignalStatus::Expected` with another if the
    /// previous waker gets expired and the caller wants to be woken again.
    pub fn wait_for_signals(&mut self) -> Rc<RefCell<SignalStatus>> {
        let status = Rc::new(RefCell::new(SignalStatus::Expected(None)));
        self.awaiters.push(Rc::downgrade(&status));
        status
    }

    /// Wakes awaiters for caught signals.
    ///
    /// This function wakes up all wakers in pending `SignalStatus`es and
    /// removes them from `self`.
    ///
    /// This function borrows `SignalStatus`es returned from `wait_for_signals`
    /// so you must not have conflicting borrows.
    pub fn wake(&mut self, signals: &Rc<[Signal]>) {
        for status in std::mem::take(&mut self.awaiters) {
            if let Some(status) = status.upgrade() {
                let mut status_ref = status.borrow_mut();
                let new_status = SignalStatus::Caught(Rc::clone(signals));
                let old_status = std::mem::replace(&mut *status_ref, new_status);
                drop(status_ref);
                if let SignalStatus::Expected(Some(waker)) = old_status {
                    waker.wake();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::r#virtual::VirtualSystem;
    use crate::system::r#virtual::PIPE_SIZE;
    use assert_matches::assert_matches;
    use futures_util::task::noop_waker;
    use futures_util::task::noop_waker_ref;
    use futures_util::FutureExt;
    use std::future::Future;
    use std::rc::Rc;
    use std::task::Context;

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
            .set_signal_handling(Signal::SIGCHLD, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(Signal::SIGINT, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(Signal::SIGUSR1, SignalHandling::Catch)
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.wait_for_signals());
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            assert!(process.blocked_signals().contains(&Signal::SIGCHLD));
            assert!(process.blocked_signals().contains(&Signal::SIGINT));
            assert!(process.blocked_signals().contains(&Signal::SIGUSR1));
            let _ = process.raise_signal(Signal::SIGCHLD);
            let _ = process.raise_signal(Signal::SIGINT);
        }
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        system.select(false).unwrap();
        let result = future.as_mut().poll(&mut context);
        assert_matches!(result, Poll::Ready(signals) => {
            assert_eq!(signals.len(), 2);
            assert!(signals.contains(&Signal::SIGCHLD));
            assert!(signals.contains(&Signal::SIGINT));
        });
    }

    #[test]
    fn shared_system_wait_for_signal_returns_on_caught() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        system
            .set_signal_handling(Signal::SIGCHLD, SignalHandling::Catch)
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.wait_for_signal(Signal::SIGCHLD));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            assert!(process.blocked_signals().contains(&Signal::SIGCHLD));
            let _ = process.raise_signal(Signal::SIGCHLD);
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
            .set_signal_handling(Signal::SIGINT, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(Signal::SIGTERM, SignalHandling::Catch)
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.wait_for_signal(Signal::SIGINT));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            let _ = process.raise_signal(Signal::SIGCHLD);
            let _ = process.raise_signal(Signal::SIGTERM);
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
            .set_signal_handling(Signal::SIGINT, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(Signal::SIGTERM, SignalHandling::Catch)
            .unwrap();

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            let _ = process.raise_signal(Signal::SIGINT);
            let _ = process.raise_signal(Signal::SIGTERM);
        }
        system.select(false).unwrap();

        let state = state.borrow();
        let process = state.processes.get(&process_id).unwrap();
        let blocked = process.blocked_signals();
        assert!(blocked.contains(&Signal::SIGINT));
        assert!(blocked.contains(&Signal::SIGTERM));
        let pending = process.pending_signals();
        assert!(!pending.contains(&Signal::SIGINT));
        assert!(!pending.contains(&Signal::SIGTERM));
    }

    #[test]
    fn shared_system_select_does_not_wake_signal_waiters_on_io() {
        let system = VirtualSystem::new();
        let mut system_1 = SharedSystem::new(Box::new(system));
        let mut system_2 = system_1.clone();
        let mut system_3 = system_1.clone();
        let (reader, writer) = system_1.pipe().unwrap();
        system_2
            .set_signal_handling(Signal::SIGCHLD, SignalHandling::Catch)
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

    #[test]
    fn async_io_has_no_default_readers_or_writers() {
        let async_io = AsyncIo::new();
        assert_eq!(async_io.readers(), FdSet::new());
        assert_eq!(async_io.writers(), FdSet::new());
    }

    #[test]
    fn async_io_non_empty_readers_and_writers() {
        let mut async_io = AsyncIo::new();
        let waker = Rc::new(RefCell::new(Some(noop_waker())));
        async_io.wait_for_reading(Fd::STDIN, &waker);
        async_io.wait_for_writing(Fd::STDOUT, &waker);
        async_io.wait_for_writing(Fd::STDERR, &waker);

        let mut expected_readers = FdSet::new();
        expected_readers.insert(Fd::STDIN).unwrap();
        let mut expected_writers = FdSet::new();
        expected_writers.insert(Fd::STDOUT).unwrap();
        expected_writers.insert(Fd::STDERR).unwrap();
        assert_eq!(async_io.readers(), expected_readers);
        assert_eq!(async_io.writers(), expected_writers);
    }

    #[test]
    fn async_io_wake() {
        let mut async_io = AsyncIo::new();
        let waker = Rc::new(RefCell::new(Some(noop_waker())));
        async_io.wait_for_reading(Fd(3), &waker);
        async_io.wait_for_reading(Fd(4), &waker);
        async_io.wait_for_writing(Fd(4), &waker);
        let mut fds = FdSet::new();
        fds.insert(Fd(4)).unwrap();
        async_io.wake(fds, fds);

        let mut expected_readers = FdSet::new();
        expected_readers.insert(Fd(3)).unwrap();
        assert_eq!(async_io.readers(), expected_readers);
        assert_eq!(async_io.writers(), FdSet::new());
    }

    #[test]
    fn async_io_wake_all() {
        let mut async_io = AsyncIo::new();
        let waker = Rc::new(RefCell::new(Some(noop_waker())));
        async_io.wait_for_reading(Fd::STDIN, &waker);
        async_io.wait_for_writing(Fd::STDOUT, &waker);
        async_io.wait_for_writing(Fd::STDERR, &waker);
        async_io.wake_all();
        assert_eq!(async_io.readers(), FdSet::new());
        assert_eq!(async_io.writers(), FdSet::new());
    }

    #[test]
    fn async_time_first_target() {
        let mut async_time = AsyncTime::new();
        let now = Instant::now();
        assert_eq!(async_time.first_target(), None);

        async_time.push(Timeout {
            target: now + Duration::from_secs(2),
            waker: Weak::default(),
        });
        async_time.push(Timeout {
            target: now + Duration::from_secs(1),
            waker: Weak::default(),
        });
        async_time.push(Timeout {
            target: now + Duration::from_secs(3),
            waker: Weak::default(),
        });
        assert_eq!(
            async_time.first_target(),
            Some(now + Duration::from_secs(1))
        );
    }

    #[test]
    fn async_time_wake_if_passed() {
        let mut async_time = AsyncTime::new();
        let now = Instant::now();
        let waker = Rc::new(RefCell::new(Some(noop_waker())));
        async_time.push(Timeout {
            target: now,
            waker: Rc::downgrade(&waker),
        });
        async_time.push(Timeout {
            target: now + Duration::new(1, 0),
            waker: Rc::downgrade(&waker),
        });
        async_time.push(Timeout {
            target: now + Duration::new(1, 1),
            waker: Rc::downgrade(&waker),
        });
        async_time.push(Timeout {
            target: now + Duration::new(2, 0),
            waker: Rc::downgrade(&waker),
        });
        assert_eq!(async_time.timeouts.len(), 4);

        async_time.wake_if_passed(now + Duration::new(1, 0));
        assert_eq!(
            async_time.timeouts.pop().unwrap().0.target,
            now + Duration::new(1, 1)
        );
        assert_eq!(
            async_time.timeouts.pop().unwrap().0.target,
            now + Duration::new(2, 0)
        );
        assert!(async_time.timeouts.is_empty(), "{:?}", async_time.timeouts);
    }

    #[test]
    fn async_signal_wake() {
        let mut async_signal = AsyncSignal::new();
        let status_1 = async_signal.wait_for_signals();
        let status_2 = async_signal.wait_for_signals();
        *status_1.borrow_mut() = SignalStatus::Expected(Some(noop_waker()));
        *status_2.borrow_mut() = SignalStatus::Expected(Some(noop_waker()));

        async_signal.wake(&(Rc::new([Signal::SIGCHLD, Signal::SIGUSR1]) as Rc<[Signal]>));
        assert_matches!(&*status_1.borrow(), SignalStatus::Caught(signals) => {
            assert_eq!(**signals, [Signal::SIGCHLD, Signal::SIGUSR1]);
        });
        assert_matches!(&*status_2.borrow(), SignalStatus::Caught(signals) => {
            assert_eq!(**signals, [Signal::SIGCHLD, Signal::SIGUSR1]);
        });
    }
}
