// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! Items related to process management

use super::{ExitStatus, Result, Signals};
#[cfg(all(doc, unix))]
use crate::RealSystem;
#[cfg(doc)]
use crate::VirtualSystem;
use crate::job::Pid;
use crate::job::ProcessState;
use std::convert::Infallible;
use std::ffi::{CStr, CString};
use std::rc::Rc;

/// Trait for getting the current process ID and other process-related information
pub trait GetPid {
    /// Returns the process ID of the current process.
    ///
    /// This method represents the [`getpid` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/getpid.html).
    #[must_use]
    fn getpid(&self) -> Pid;

    /// Returns the process ID of the parent process.
    ///
    /// This method represents the [`getppid` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/getppid.html).
    #[must_use]
    fn getppid(&self) -> Pid;

    /// Returns the process group ID of the current process.
    ///
    /// This method represents the [`getpgrp` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/getpgrp.html).
    #[must_use]
    fn getpgrp(&self) -> Pid;

    /// Returns the session ID of the specified process.
    ///
    /// If `pid` is `Pid(0)`, this function returns the session ID of the
    /// current process.
    fn getsid(&self, pid: Pid) -> Result<Pid>;
}

/// Delegates the `GetPid` trait to the contained instance of `S`
impl<S: GetPid> GetPid for Rc<S> {
    #[inline]
    fn getpid(&self) -> Pid {
        (self as &S).getpid()
    }
    #[inline]
    fn getppid(&self) -> Pid {
        (self as &S).getppid()
    }
    #[inline]
    fn getpgrp(&self) -> Pid {
        (self as &S).getpgrp()
    }
    #[inline]
    fn getsid(&self, pid: Pid) -> Result<Pid> {
        (self as &S).getsid(pid)
    }
}

/// Trait for modifying the process group ID of processes
pub trait SetPgid {
    /// Modifies the process group ID of a process.
    ///
    /// This method represents the [`setpgid` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/setpgid.html).
    ///
    /// `pid` specifies the process whose process group ID is to be changed. If `pid` is
    /// `Pid(0)`, the current process is used.
    /// `pgid` specifies the new process group ID to be set. If `pgid` is
    /// `Pid(0)`, the process ID of the specified process is used.
    fn setpgid(&self, pid: Pid, pgid: Pid) -> Result<()>;
}

/// Delegates the `SetPgid` trait to the contained instance of `S`
impl<S: SetPgid> SetPgid for Rc<S> {
    #[inline]
    fn setpgid(&self, pid: Pid, pgid: Pid) -> Result<()> {
        (self as &S).setpgid(pid, pgid)
    }
}

/// Trait for spawning new processes
pub trait Fork {
    /// Runs a task in a new child process.
    ///
    /// This is a low-level wrapper around the [`fork` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fork.html).
    /// You should generally use [`Subshell`](crate::subshell::Subshell) instead
    /// of this method to create a subshell, so that the environment can
    /// condition the state of the child process before it starts running.
    ///
    /// There are two notable differences from the standard `fork` system call:
    ///
    /// 1. This method takes a task to be executed in the child process, and
    ///    does not return in the child process. This makes the
    ///    [`VirtualSystem`] implementation easier, as it does not need to copy
    ///    the calling stack to create a new virtual process.
    /// 2. This method allows passing shared data between the parent and child
    ///    processes. In the parent process, the data is returned intact along
    ///    with the child process ID. In the child process, the data is passed
    ///    to the task, which may be a clone of the original data in
    ///    [`VirtualSystem`] that runs the task in the same real process.
    ///
    /// The asynchronicity of the child task must be used only for simulating
    /// process suspension and abortion in [`VirtualSystem`]. In a real child
    /// process created by [`RealSystem`], the task must run to completion
    /// without yielding (that is, [`Future::poll`] must not return
    /// `Poll::Pending`).
    ///
    /// This trait expects the child task to terminate itself by calling
    /// [`Exit::exit`] or killing itself with a signal. If the task returns
    /// without doing so, the behavior is unspecified.
    fn run_in_child_process<D, F>(&self, shared_data: D, child_task: F) -> (Result<Pid>, D)
    where
        Self: Sized,
        D: Clone + 'static,
        F: AsyncFnOnce(Self, D) + 'static;
}

/// Trait for waiting for child processes
pub trait Wait: Signals {
    /// Reports updated status of a child process.
    ///
    /// This is a low-level function used internally by
    /// [`Env::wait_for_subshell`](crate::Env::wait_for_subshell). You should
    /// not call this function directly, or you will disrupt the behavior of
    /// `Env`. The description below applies if you want to do everything
    /// yourself without depending on `Env`.
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
    fn wait(&self, target: Pid) -> Result<Option<(Pid, ProcessState)>>;
}

/// Delegates the `Wait` trait to the contained instance of `S`
impl<S: Wait> Wait for Rc<S> {
    #[inline]
    fn wait(&self, target: Pid) -> Result<Option<(Pid, ProcessState)>> {
        (self as &S).wait(target)
    }
}

/// Trait for executing a new program in the current process
pub trait Exec {
    // TODO Consider passing raw pointers for optimization
    /// Replaces the current process with an external utility.
    ///
    /// This is a thin wrapper around the `execve` system call.
    fn execve(
        &self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> impl Future<Output = Result<Infallible>> + use<Self>;
}

/// Delegates the `Exec` trait to the contained instance of `S`
impl<S: Exec> Exec for Rc<S> {
    #[inline]
    fn execve(
        &self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> impl Future<Output = Result<Infallible>> + use<S> {
        (self as &S).execve(path, args, envs)
    }
}

/// Trait for terminating the current process
pub trait Exit {
    /// Terminates the current process.
    ///
    /// This function is a thin wrapper around the [`_exit` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/_exit.html).
    fn exit(&self, exit_status: ExitStatus) -> impl Future<Output = Infallible> + use<Self>;
}

/// Delegates the `Exit` trait to the contained instance of `S`
impl<S: Exit> Exit for Rc<S> {
    #[inline]
    fn exit(&self, exit_status: ExitStatus) -> impl Future<Output = Infallible> + use<S> {
        (self as &S).exit(exit_status)
    }
}
