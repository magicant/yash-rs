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
use crate::Env;
#[cfg(all(doc, unix))]
use crate::RealSystem;
#[cfg(doc)]
use crate::VirtualSystem;
use crate::job::Pid;
use crate::job::ProcessState;
use std::convert::Infallible;
use std::ffi::{CStr, CString};
use std::pin::Pin;

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

type PinFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Task executed in a child process
///
/// This is an argument passed to a [`ChildProcessStarter`]. The task is
/// executed in a child process initiated by the starter. The environment passed
/// to the task is a clone of the parent environment, but it has a different
/// process ID than the parent.
///
/// Note that the output type of the task is `Infallible`. This is to ensure
/// that the task [exits](super::Exit::exit) cleanly or
/// [kills](super::SendSignal::kill) itself with a signal.
pub type ChildProcessTask<S> = Box<dyn for<'a> FnOnce(&'a mut Env<S>) -> PinFuture<'a, Infallible>>;

/// Abstract function that starts a child process
///
/// [`Fork::new_child_process`] returns a child process starter. You need to
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
/// [`VirtualSystem::new_child_process`] does not create a real child process.
/// Instead, the starter runs the task concurrently in the current process using
/// the executor contained in the system. A new
/// [`Process`](super::virtual::Process) is added to the system to represent the
/// child process. The starter returns its process ID.
///
/// This function only starts the child, which continues to run asynchronously
/// after the function returns its PID. To wait for the child to finish and
/// obtain its exit status, use [`wait`](Wait::wait).
pub type ChildProcessStarter<S> = Box<dyn FnOnce(&mut Env<S>, ChildProcessTask<S>) -> Pid>;

/// Trait for spawning new processes
pub trait Fork {
    // XXX: This method needs refactoring! (#662)
    /// Creates a new child process.
    ///
    /// This is a wrapper around the [`fork` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fork.html).
    /// Users of [`Env`] should not call it directly. Instead, use
    /// [`Subshell`](crate::subshell::Subshell) so that the environment can
    /// condition the state of the child process before it starts running.
    ///
    /// Because we need the parent environment to create the child environment,
    /// this method cannot initiate the child task directly. Instead, it returns
    /// a [`ChildProcessStarter`] function that takes the parent environment and
    /// the child task. The caller must call the starter to make sure the parent
    /// and child processes perform correctly after forking.
    fn new_child_process(&self) -> Result<ChildProcessStarter<Self>>
    where
        Self: Sized;
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

/// Trait for terminating the current process
pub trait Exit {
    /// Terminates the current process.
    ///
    /// This function is a thin wrapper around the [`_exit` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/_exit.html).
    fn exit(&self, exit_status: ExitStatus) -> impl Future<Output = Infallible> + use<Self>;
}
