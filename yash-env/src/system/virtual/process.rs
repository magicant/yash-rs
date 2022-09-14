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
use crate::io::Fd;
use crate::semantics::ExitStatus;
use crate::system::SelectSystem;
use crate::SignalHandling;
use nix::sys::signal::SigSet;
use nix::sys::signal::SigmaskHow;
use nix::sys::signal::Signal;
use nix::sys::wait::WaitStatus;
use nix::unistd::Pid;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ffi::CString;
use std::fmt::Debug;
use std::os::unix::io::RawFd;
use std::rc::Weak;

/// Process in a virtual system.
#[derive(Clone, Debug)]
pub struct Process {
    /// Process ID of the parent process.
    pub(crate) ppid: Pid,

    /// Set of file descriptors open in this process.
    pub(crate) fds: BTreeMap<Fd, FdBody>,

    /// Execution state of the process.
    pub(crate) state: ProcessState,

    /// True when `state` has changed but not yet reported to the parent
    /// process.
    ///
    /// The change of `state` is reported when the parent `wait`s for this
    /// process.
    state_has_changed: bool,

    /// Currently set signal handlers.
    ///
    /// For signals not contained in this hash map, the default handler is
    /// assumed.
    signal_handlings: HashMap<Signal, SignalHandling>,

    /// Set of blocked signals.
    blocked_signals: SigSet,

    /// Set of pending signals.
    pending_signals: SigSet,

    /// List of signals that have been delivered and caught.
    pub(crate) caught_signals: Vec<Signal>,

    /// Maximum number of open file descriptors.
    pub(crate) rlimit_nofile: RawFd,

    /// Weak reference to the `SelectSystem` for this process.
    ///
    /// This weak reference is empty for the initial process of a
    /// `VirtualSystem`.  When a new child process is created, a weak reference
    /// to the `SelectSystem` for the child is set.
    pub(crate) selector: Weak<RefCell<SelectSystem>>,

    /// Copy of arguments passed to [`execve`](crate::System::execve).
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
            state_has_changed: false,
            signal_handlings: HashMap::new(),
            blocked_signals: SigSet::empty(),
            pending_signals: SigSet::empty(),
            caught_signals: Vec::new(),
            rlimit_nofile: 1 << 10,
            selector: Weak::new(),
            last_exec: None,
        }
    }

    /// Creates a new running process as a child of the given parent.
    ///
    /// Some part of the parent process state is copied to the new process.
    pub fn fork_from(ppid: Pid, parent: &Process) -> Process {
        let mut child = Self::with_parent(ppid);
        child.fds = parent.fds.clone();
        child.signal_handlings = parent.signal_handlings.clone();
        child.blocked_signals = parent.blocked_signals;
        child.pending_signals = parent.pending_signals;
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
    pub fn get_fd(&self, fd: Fd) -> Option<&FdBody> {
        self.fds.get(&fd)
    }

    /// Returns the body for the given FD.
    #[inline]
    #[must_use]
    pub fn get_fd_mut(&mut self, fd: Fd) -> Option<&mut FdBody> {
        self.fds.get_mut(&fd)
    }

    /// Assigns the given FD to the body.
    ///
    /// If successful, returns an `Ok` value containing the previous body for
    /// the FD. If the FD is equal to or greater than `rlimit_nofile`, returns
    /// `Err(body)`.
    pub fn set_fd(&mut self, fd: Fd, body: FdBody) -> Result<Option<FdBody>, FdBody> {
        if fd.0 < self.rlimit_nofile {
            Ok(self.fds.insert(fd, body))
        } else {
            Err(body)
        }
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

    /// Removes all FD bodies in this process.
    pub fn close_fds(&mut self) {
        self.fds.clear();
    }

    /// Returns the process state.
    #[inline(always)]
    #[must_use]
    pub fn state(&self) -> ProcessState {
        self.state
    }

    /// Sets the state of this process.
    ///
    /// If the new state is `Exited` or `Signaled`, all file descriptors in this
    /// process are closed.
    ///
    /// This function returns whether the state did change. If true, the
    /// [`state_has_changed`](Self::state_has_changed) flag is set and  the
    /// caller must notify the status update by sending `SIGCHLD` to the parent
    /// process.
    #[must_use = "You must send SIGCHLD to the parent"]
    pub fn set_state(&mut self, state: ProcessState) -> bool {
        let old_state = std::mem::replace(&mut self.state, state);

        if old_state == state {
            false
        } else {
            match state {
                ProcessState::Exited(_) | ProcessState::Signaled(_) => self.close_fds(),
                _ => (),
            }
            self.state_has_changed = true;
            true
        }
    }

    /// Returns true if a new state has been [set](Self::set_state) but not yet
    /// [taken](Self::take_state).
    #[must_use]
    pub fn state_has_changed(&self) -> bool {
        self.state_has_changed
    }

    /// Returns the process state and clears the
    /// [`state_has_changed`](Self::state_has_changed) flag.
    pub fn take_state(&mut self) -> ProcessState {
        self.state_has_changed = false;
        self.state
    }

    /// Returns the currently blocked signals.
    pub fn blocked_signals(&self) -> &SigSet {
        &self.blocked_signals
    }

    /// Returns the currently pending signals.
    ///
    /// A signal is pending when it has been raised but not yet delivered
    /// because it is being blocked.
    pub fn pending_signals(&self) -> &SigSet {
        &self.pending_signals
    }

    /// Updates the signal blocking mask for this process.
    ///
    /// If this function unblocks a signal, any pending signal is delivered.
    ///
    /// Returns true if any signal was delivered.
    pub fn block_signals(&mut self, how: SigmaskHow, signals: &SigSet) -> bool {
        match how {
            SigmaskHow::SIG_SETMASK => self.blocked_signals = *signals,
            SigmaskHow::SIG_BLOCK => self.blocked_signals.extend(signals),
            SigmaskHow::SIG_UNBLOCK => {
                for signal in Signal::iterator() {
                    if signals.contains(signal) {
                        self.blocked_signals.remove(signal);
                    }
                }
            }
            _ => unreachable!(),
        }

        let mut delivered = false;
        for signal in Signal::iterator() {
            if self.pending_signals.contains(signal) && !self.blocked_signals.contains(signal) {
                self.pending_signals.remove(signal);
                self.deliver_signal(signal);
                delivered = true;
            }
        }
        delivered
    }

    /// Returns the current handler for a signal.
    pub fn signal_handling(&self, signal: Signal) -> SignalHandling {
        self.signal_handlings
            .get(&signal)
            .copied()
            .unwrap_or_default()
    }

    /// Gets and sets the handler for a signal.
    ///
    /// This function sets the handler to `handling` and returns the previous
    /// handler.
    pub fn set_signal_handling(
        &mut self,
        signal: Signal,
        handling: SignalHandling,
    ) -> SignalHandling {
        let old_handling = self.signal_handlings.insert(signal, handling);
        old_handling.unwrap_or_default()
    }

    /// Delivers a signal to this process.
    ///
    /// The action taken on the delivery depends on the current signal handling
    /// for the signal.
    fn deliver_signal(&mut self, signal: Signal) {
        let handling = if signal == Signal::SIGKILL || signal == Signal::SIGSTOP {
            SignalHandling::Default
        } else {
            self.signal_handling(signal)
        };
        match handling {
            SignalHandling::Default => (), // TODO Do something depending on the signal
            SignalHandling::Ignore => (),
            SignalHandling::Catch => self.caught_signals.push(signal),
        }
        // TODO If the Default action changes the process state, the parent
        // should receive a SIGCHLD.
    }

    /// Sends a signal to this process.
    ///
    /// If the signal is being blocked, it will remain pending. Otherwise, it is
    /// immediately delivered.
    pub fn raise_signal(&mut self, signal: Signal) {
        if signal != Signal::SIGKILL
            && signal != Signal::SIGSTOP
            && self.blocked_signals().contains(signal)
        {
            self.pending_signals.add(signal)
        } else {
            self.deliver_signal(signal)
        }
    }

    /// Returns the arguments to the last call to
    /// [`execve`](crate::System::execve) on this process.
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
    /// Whether the process is not yet terminated.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        match self {
            ProcessState::Running | ProcessState::Stopped(_) => true,
            ProcessState::Exited(_) | ProcessState::Signaled(_) => false,
        }
    }

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
    use crate::system::r#virtual::file_system::{FileBody, INode, Mode};
    use crate::system::r#virtual::io::OpenFileDescription;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

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

    fn process_with_pipe() -> (Process, Fd, Fd) {
        let mut process = Process::with_parent(Pid::from_raw(10));

        let file = Rc::new(RefCell::new(INode {
            body: FileBody::Fifo {
                content: VecDeque::new(),
                readers: 1,
                writers: 1,
            },
            permissions: Mode::default(),
        }));
        let reader = OpenFileDescription {
            file: Rc::clone(&file),
            offset: 0,
            is_readable: true,
            is_writable: false,
            is_appending: false,
        };
        let writer = OpenFileDescription {
            file: Rc::clone(&file),
            offset: 0,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };

        let reader = FdBody {
            open_file_description: Rc::new(RefCell::new(reader)),
            cloexec: false,
        };
        let writer = FdBody {
            open_file_description: Rc::new(RefCell::new(writer)),
            cloexec: false,
        };

        let reader = process.open_fd(reader).unwrap();
        let writer = process.open_fd(writer).unwrap();
        (process, reader, writer)
    }

    #[test]
    fn process_set_state_closes_all_fds_on_exit() {
        let (mut process, _reader, _writer) = process_with_pipe();
        assert!(process.set_state(ProcessState::Exited(ExitStatus(3))));
        assert!(process.fds().is_empty(), "{:?}", process.fds());
    }

    #[test]
    fn process_set_state_closes_all_fds_on_signaled() {
        let (mut process, _reader, _writer) = process_with_pipe();
        assert!(process.set_state(ProcessState::Signaled(Signal::SIGINT)));
        assert!(process.fds().is_empty(), "{:?}", process.fds());
    }

    #[test]
    fn process_default_signal_blocking_mask() {
        let process = Process::with_parent(Pid::from_raw(10));
        let initial_set = process.blocked_signals();
        for signal in Signal::iterator() {
            assert!(!initial_set.contains(signal), "contained signal {}", signal);
        }
    }

    #[test]
    fn process_sigmask_setmask() {
        let mut process = Process::with_parent(Pid::from_raw(10));
        let mut some_set = SigSet::empty();
        some_set.add(Signal::SIGINT);
        some_set.add(Signal::SIGCHLD);
        let delivered = process.block_signals(SigmaskHow::SIG_SETMASK, &some_set);
        assert!(!delivered);

        let result_set = process.blocked_signals();
        // TODO assert_eq!(result_set, some_set);
        assert!(result_set.contains(Signal::SIGINT));
        assert!(result_set.contains(Signal::SIGCHLD));

        some_set.clear();
        some_set.add(Signal::SIGINT);
        some_set.add(Signal::SIGQUIT);
        let delivered = process.block_signals(SigmaskHow::SIG_SETMASK, &some_set);
        assert!(!delivered);

        let result_set = process.blocked_signals();
        assert!(result_set.contains(Signal::SIGINT));
        assert!(result_set.contains(Signal::SIGQUIT));
        assert!(!result_set.contains(Signal::SIGCHLD));
    }

    #[test]
    fn process_sigmask_block() {
        let mut process = Process::with_parent(Pid::from_raw(10));
        let mut some_set = SigSet::empty();
        some_set.add(Signal::SIGINT);
        some_set.add(Signal::SIGCHLD);
        let delivered = process.block_signals(SigmaskHow::SIG_BLOCK, &some_set);
        assert!(!delivered);

        let result_set = process.blocked_signals();
        // TODO assert_eq!(result_set, some_set);
        assert!(result_set.contains(Signal::SIGINT));
        assert!(result_set.contains(Signal::SIGCHLD));

        some_set.clear();
        some_set.add(Signal::SIGINT);
        some_set.add(Signal::SIGQUIT);
        let delivered = process.block_signals(SigmaskHow::SIG_BLOCK, &some_set);
        assert!(!delivered);

        let result_set = process.blocked_signals();
        assert!(result_set.contains(Signal::SIGINT));
        assert!(result_set.contains(Signal::SIGQUIT));
        assert!(result_set.contains(Signal::SIGCHLD));
    }

    #[test]
    fn process_sigmask_unblock() {
        let mut process = Process::with_parent(Pid::from_raw(10));
        let mut some_set = SigSet::empty();
        some_set.add(Signal::SIGINT);
        some_set.add(Signal::SIGCHLD);
        let delivered = process.block_signals(SigmaskHow::SIG_BLOCK, &some_set);
        assert!(!delivered);

        some_set.clear();
        some_set.add(Signal::SIGINT);
        some_set.add(Signal::SIGQUIT);
        let delivered = process.block_signals(SigmaskHow::SIG_UNBLOCK, &some_set);
        assert!(!delivered);

        let result_set = process.blocked_signals();
        assert!(!result_set.contains(Signal::SIGINT));
        assert!(!result_set.contains(Signal::SIGQUIT));
        assert!(result_set.contains(Signal::SIGCHLD));
    }

    #[test]
    fn process_set_signal_handling() {
        let mut process = Process::with_parent(Pid::from_raw(100));
        let old_handling = process.set_signal_handling(Signal::SIGINT, SignalHandling::Ignore);
        assert_eq!(old_handling, SignalHandling::Default);
        let old_handling = process.set_signal_handling(Signal::SIGTERM, SignalHandling::Catch);
        assert_eq!(old_handling, SignalHandling::Default);

        let old_handling = process.set_signal_handling(Signal::SIGINT, SignalHandling::Default);
        assert_eq!(old_handling, SignalHandling::Ignore);
        let old_handling = process.set_signal_handling(Signal::SIGTERM, SignalHandling::Ignore);
        assert_eq!(old_handling, SignalHandling::Catch);

        let handling = process.signal_handling(Signal::SIGINT);
        assert_eq!(handling, SignalHandling::Default);
        let handling = process.signal_handling(Signal::SIGTERM);
        assert_eq!(handling, SignalHandling::Ignore);
        let handling = process.signal_handling(Signal::SIGQUIT);
        assert_eq!(handling, SignalHandling::Default);
    }

    #[test]
    fn process_raise_signal_default_nop() {
        let mut process = Process::with_parent(Pid::from_raw(42));
        process.raise_signal(Signal::SIGCHLD);
        assert_eq!(process.state(), ProcessState::Running);
    }

    #[test]
    #[ignore] // TODO enable this test case
    fn process_raise_signal_default_terminating() {
        let mut process = Process::with_parent(Pid::from_raw(42));
        process.raise_signal(Signal::SIGTERM);
        assert_eq!(process.state(), ProcessState::Signaled(Signal::SIGTERM));
        assert_eq!(process.caught_signals, []);
    }

    // TODO process_raise_signal_default_aborting
    // TODO process_raise_signal_default_stopping
    // TODO process_raise_signal_default_continuing

    #[test]
    fn process_raise_signal_ignored() {
        let mut process = Process::with_parent(Pid::from_raw(42));
        process.set_signal_handling(Signal::SIGCHLD, SignalHandling::Ignore);
        process.raise_signal(Signal::SIGCHLD);
        assert_eq!(process.state(), ProcessState::Running);
        assert_eq!(process.caught_signals, []);
    }

    #[test]
    fn process_raise_signal_caught() {
        let mut process = Process::with_parent(Pid::from_raw(42));
        process.set_signal_handling(Signal::SIGCHLD, SignalHandling::Catch);
        process.raise_signal(Signal::SIGCHLD);
        assert_eq!(process.state(), ProcessState::Running);
        assert_eq!(process.caught_signals, [Signal::SIGCHLD]);
    }

    fn to_set<I: IntoIterator<Item = Signal>>(signals: I) -> SigSet {
        let mut set = SigSet::empty();
        // TODO set.extend(signals)
        for signal in signals {
            set.add(signal);
        }
        set
    }

    #[test]
    fn process_raise_signal_blocked() {
        let mut process = Process::with_parent(Pid::from_raw(42));
        process.set_signal_handling(Signal::SIGCHLD, SignalHandling::Catch);
        process.block_signals(SigmaskHow::SIG_BLOCK, &to_set([Signal::SIGCHLD]));
        process.raise_signal(Signal::SIGCHLD);
        assert_eq!(process.state(), ProcessState::Running);
        assert_eq!(process.caught_signals, []);

        let delivered = process.block_signals(SigmaskHow::SIG_SETMASK, &SigSet::empty());
        assert!(delivered);
        assert_eq!(process.state(), ProcessState::Running);
        assert_eq!(process.caught_signals, [Signal::SIGCHLD]);
    }
}
