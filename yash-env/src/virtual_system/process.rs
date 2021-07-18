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

//! Processes in a virtual system.

use crate::exec::ExitStatus;
use nix::sys::signal::Signal;
use nix::sys::wait::WaitStatus;
use nix::unistd::Pid;
use std::ffi::CString;
use std::fmt::Debug;
use std::task::Waker;

/// Process in a virtual system.
#[derive(Clone, Debug)]
pub struct Process {
    /// Process ID of the parent process.
    pub(crate) ppid: Pid,

    /// State of the process.
    pub(crate) state: ProcessState,

    /// References to tasks that are waiting for the process state to change.
    ///
    /// If this is `None`, the `state` has changed but not yet been reported by
    /// the `wait` system call. The next `wait` call should immediately notify
    /// the current state. If this is `Some(_)`, the `state` has not changed
    /// since the last `wait` call. The next `wait` call should leave a waker
    /// so that the caller is woken when the state changes later.
    pub(crate) state_awaiters: Option<Vec<Waker>>,

    /// Copy of arguments passed to [`execve`](crate::VirtualSystem::execve).
    pub(crate) last_exec: Option<(CString, Vec<CString>, Vec<CString>)>,
}

impl Process {
    /// Creates a new running process.
    pub fn with_parent(ppid: Pid) -> Process {
        Process {
            ppid,
            state: ProcessState::Running,
            state_awaiters: Some(Vec::new()),
            last_exec: None,
        }
    }

    /// Returns the process ID of the parent process.
    #[inline(always)]
    #[must_use]
    pub fn ppid(&self) -> Pid {
        self.ppid
    }

    /// Returns the process state.
    #[inline(always)]
    #[must_use]
    pub fn state(&self) -> ProcessState {
        self.state
    }

    /// Sets the state of this process.
    ///
    /// This function returns wakers that must be woken. The caller must first
    /// drop the `RefMut` borrowing the [`SystemState`](super::SystemState) containing this
    /// `Process` and then wake the wakers returned from this function. This is
    /// to prevent a possible second borrow by another task.
    #[must_use = "You must wake up the returned waker"]
    pub fn set_state(&mut self, state: ProcessState) -> Vec<Waker> {
        let old_state = std::mem::replace(&mut self.state, state);

        if old_state == state {
            Vec::new()
        } else {
            self.state_awaiters.take().unwrap_or_else(Vec::new)
        }
    }

    /// Returns the arguments to the last call to
    /// [`execve`](crate::VirtualSystem::execve) on this process.
    #[inline(always)]
    #[must_use]
    pub fn last_exec(&self) -> &Option<(CString, Vec<CString>, Vec<CString>)> {
        &self.last_exec
    }
}

/// State of a process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    Running,
    Stopped(Signal),
    Exited(ExitStatus),
    Signaled(Signal),
}

impl ProcessState {
    /// Converts `ProcessState` to `WaitStatus`.
    #[must_use]
    pub fn to_wait_status(self, pid: Pid) -> WaitStatus {
        match self {
            ProcessState::Running => WaitStatus::Continued(pid),
            ProcessState::Exited(exit_status) => WaitStatus::Exited(pid, exit_status.0),
            ProcessState::Stopped(signal) => WaitStatus::Stopped(pid, signal),
            ProcessState::Signaled(signal) => WaitStatus::Signaled(pid, signal, false),
        }
    }
}
