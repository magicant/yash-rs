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

use super::io::FdBody;
use crate::exec::ExitStatus;
use crate::io::Fd;
use nix::sys::signal::Signal;
use nix::sys::wait::WaitStatus;
use nix::unistd::Pid;
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fmt::Debug;
use std::task::Waker;

/// Process in a virtual system.
#[derive(Clone, Debug)]
pub struct Process {
    /// Process ID of the parent process.
    pub(crate) ppid: Pid,

    /// Set of file descriptors open in this process.
    pub(crate) fds: BTreeMap<Fd, FdBody>,

    /// Execution state of the process.
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

/// Finds the minimum available FD.
///
/// The returned FD is the minimum that is equal to or greater than `min` and
/// not included in `existings`. Items of `existings` must be sorted.
fn min_unused_fd<'a, I: IntoIterator<Item = &'a Fd>>(min: Fd, existings: I) -> Fd {
    let candidates = (min.0..).map(Fd);
    let rejections = existings
        .into_iter()
        .skip_while(|fd| **fd < min)
        .map(Some)
        .chain(std::iter::repeat(None));
    candidates
        .zip(rejections)
        .skip_while(|(candidate, rejection)| Some(candidate) == *rejection)
        .map(|(candidate, _rejection)| candidate)
        .next()
        .unwrap()
}

impl Process {
    /// Creates a new running process.
    pub fn with_parent(ppid: Pid) -> Process {
        Process {
            ppid,
            fds: BTreeMap::new(),
            state: ProcessState::Running,
            state_awaiters: Some(Vec::new()),
            last_exec: None,
        }
    }

    /// Creates a new running process as a child of the given parent.
    ///
    /// Some part of the parent process state is copied to the new process.
    pub fn fork_from(ppid: Pid, parent: &Process) -> Process {
        let mut child = Self::with_parent(ppid);
        child.fds = parent.fds.clone();
        child
    }

    /// Returns the process ID of the parent process.
    #[inline(always)]
    #[must_use]
    pub fn ppid(&self) -> Pid {
        self.ppid
    }

    /// Returns FDs open in this process.
    #[inline(always)]
    #[must_use]
    pub fn fds(&self) -> &BTreeMap<Fd, FdBody> {
        &self.fds
    }

    /// Returns the body for the given FD.
    #[inline]
    #[must_use]
    pub fn get_fd_mut(&mut self, fd: Fd) -> Option<&mut FdBody> {
        self.fds.get_mut(&fd)
    }

    /// Assigns the given FD to the body.
    ///
    /// If successful, returns an `Ok` value containing the body for the FD. If
    /// the FD is out of bounds, returns `Err(body)`.
    pub fn set_fd(&mut self, fd: Fd, body: FdBody) -> Result<Option<FdBody>, FdBody> {
        // TODO fail if fd is out of bounds (cf. EMFILE)
        Ok(self.fds.insert(fd, body))
    }

    /// Assigns a new FD to the given body.
    ///
    /// The new FD will be the minimum unused FD equal to or greater than
    /// `min_fd`.
    ///
    /// If successful, the new FD is returned in `Ok`.
    /// If no more FD can be opened, returns `Err(body)`.
    pub fn open_fd_ge(&mut self, min_fd: Fd, body: FdBody) -> Result<Fd, FdBody> {
        let fd = min_unused_fd(min_fd, self.fds.keys());
        let old_body = self.set_fd(fd, body)?;
        debug_assert_eq!(old_body, None);
        Ok(fd)
    }

    /// Assigns a new FD to the given body.
    ///
    /// The new FD will be the minimum unused FD, which will be returned as an
    /// `Ok` value.
    ///
    /// If no more FD can be opened, returns `Err(body)`.
    pub fn open_fd(&mut self, body: FdBody) -> Result<Fd, FdBody> {
        self.open_fd_ge(Fd(0), body)
    }

    /// Removes the FD body for the given FD.
    pub fn close_fd(&mut self, fd: Fd) -> Option<FdBody> {
        self.fds.remove(&fd)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_unused_fd_for_various_arguments() {
        assert_eq!(min_unused_fd(Fd(0), []), Fd(0));
        assert_eq!(min_unused_fd(Fd(0), [&Fd(1)]), Fd(0));
        assert_eq!(min_unused_fd(Fd(0), [&Fd(3), &Fd(4)]), Fd(0));
        assert_eq!(min_unused_fd(Fd(0), [&Fd(0)]), Fd(1));
        assert_eq!(min_unused_fd(Fd(0), [&Fd(0), &Fd(2)]), Fd(1));
        assert_eq!(min_unused_fd(Fd(0), [&Fd(0), &Fd(1)]), Fd(2));
        assert_eq!(min_unused_fd(Fd(0), [&Fd(0), &Fd(1), &Fd(4)]), Fd(2));
        assert_eq!(min_unused_fd(Fd(0), [&Fd(0), &Fd(1), &Fd(2)]), Fd(3));

        assert_eq!(min_unused_fd(Fd(1), []), Fd(1));
        assert_eq!(min_unused_fd(Fd(1), [&Fd(1)]), Fd(2));
        assert_eq!(min_unused_fd(Fd(1), [&Fd(2)]), Fd(1));

        assert_eq!(min_unused_fd(Fd(1), [&Fd(1), &Fd(3), &Fd(4)]), Fd(2));
        assert_eq!(min_unused_fd(Fd(2), [&Fd(1), &Fd(3), &Fd(4)]), Fd(2));
        assert_eq!(min_unused_fd(Fd(3), [&Fd(1), &Fd(3), &Fd(4)]), Fd(5));
        assert_eq!(min_unused_fd(Fd(4), [&Fd(1), &Fd(3), &Fd(4)]), Fd(5));
        assert_eq!(min_unused_fd(Fd(5), [&Fd(1), &Fd(3), &Fd(4)]), Fd(5));
        assert_eq!(min_unused_fd(Fd(6), [&Fd(1), &Fd(3), &Fd(4)]), Fd(6));
    }
}
