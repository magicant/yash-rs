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
use super::signal::{self, SignalEffect};
use super::Gid;
use super::Mode;
use super::SigmaskOp;
use super::SignalHandling;
use super::Uid;
use crate::io::Fd;
use crate::job::Pid;
use crate::job::ProcessResult;
use crate::job::ProcessState;
use crate::path::Path;
use crate::path::PathBuf;
use crate::system::resource::LimitPair;
use crate::system::resource::Resource;
use crate::system::resource::INFINITY;
use crate::system::SelectSystem;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::ffi::CString;
use std::fmt::Debug;
use std::ops::BitOr;
use std::ops::BitOrAssign;
use std::rc::Weak;
use std::task::Waker;

/// Process in a virtual system
#[derive(Clone, Debug)]
pub struct Process {
    /// Process ID of the parent process
    pub(crate) ppid: Pid,

    /// Process group ID of this process
    pub(crate) pgid: Pid,

    /// Real user ID of this process
    uid: Uid,

    /// Effective user ID of this process
    euid: Uid,

    /// Real group ID of this process
    gid: Gid,

    /// Effective group ID of this process
    egid: Gid,

    /// Set of file descriptors open in this process
    pub(crate) fds: BTreeMap<Fd, FdBody>,

    /// File creation mask
    pub(crate) umask: Mode,

    /// Working directory path
    pub(crate) cwd: PathBuf,

    /// Execution state of the process
    pub(crate) state: ProcessState,

    /// True when `state` has changed but not yet reported to the parent
    /// process.
    ///
    /// The change of `state` is reported when the parent `wait`s for this
    /// process.
    state_has_changed: bool,

    /// Wakers waiting for the state of this process to change to `Running`
    resumption_awaiters: Vec<Weak<Cell<Option<Waker>>>>,

    /// Currently set signal handlers
    ///
    /// For signals not contained in this hash map, the default handler is
    /// assumed.
    signal_handlings: HashMap<signal::Number, SignalHandling>,

    /// Set of blocked signals
    blocked_signals: BTreeSet<signal::Number>,

    /// Set of pending signals
    pending_signals: BTreeSet<signal::Number>,

    /// List of signals that have been delivered and caught
    pub(crate) caught_signals: Vec<signal::Number>,

    /// Limits for system resources
    pub(crate) resource_limits: HashMap<Resource, LimitPair>,

    /// Weak reference to the `SelectSystem` for this process
    ///
    /// This weak reference is empty for the initial process of a
    /// `VirtualSystem`.  When a new child process is created, a weak reference
    /// to the `SelectSystem` for the child is set.
    pub(crate) selector: Weak<RefCell<SelectSystem>>,

    /// Copy of arguments passed to [`execve`](crate::System::execve)
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
    pub fn with_parent_and_group(ppid: Pid, pgid: Pid) -> Process {
        Process {
            ppid,
            pgid,
            uid: Uid(1),
            euid: Uid(1),
            gid: Gid(1),
            egid: Gid(1),
            fds: BTreeMap::new(),
            umask: Mode::default(),
            cwd: PathBuf::new(),
            state: ProcessState::Running,
            state_has_changed: false,
            resumption_awaiters: Vec::new(),
            signal_handlings: HashMap::new(),
            blocked_signals: BTreeSet::new(),
            pending_signals: BTreeSet::new(),
            caught_signals: Vec::new(),
            resource_limits: HashMap::new(),
            selector: Weak::new(),
            last_exec: None,
        }
    }

    /// Creates a new running process as a child of the given parent.
    ///
    /// Some part of the parent process state is copied to the new process.
    pub fn fork_from(ppid: Pid, parent: &Process) -> Process {
        let mut child = Self::with_parent_and_group(ppid, parent.pgid);
        child.uid = parent.uid;
        child.euid = parent.euid;
        child.gid = parent.gid;
        child.egid = parent.egid;
        child.fds = parent.fds.clone();
        child.signal_handlings.clone_from(&parent.signal_handlings);
        child.blocked_signals.clone_from(&parent.blocked_signals);
        child.pending_signals = BTreeSet::new();
        child
    }

    /// Returns the process ID of the parent process.
    #[inline(always)]
    #[must_use]
    pub fn ppid(&self) -> Pid {
        self.ppid
    }

    /// Returns the process group ID of this process.
    #[inline(always)]
    #[must_use]
    pub fn pgid(&self) -> Pid {
        self.pgid
    }

    /// Returns the real user ID of this process.
    #[inline(always)]
    #[must_use]
    pub fn uid(&self) -> Uid {
        self.uid
    }

    /// Sets the real user ID of this process.
    #[inline(always)]
    pub fn set_uid(&mut self, uid: Uid) {
        self.uid = uid;
    }

    /// Returns the effective user ID of this process.
    #[inline(always)]
    #[must_use]
    pub fn euid(&self) -> Uid {
        self.euid
    }

    /// Sets the effective user ID of this process.
    #[inline(always)]
    pub fn set_euid(&mut self, euid: Uid) {
        self.euid = euid;
    }

    /// Returns the real group ID of this process.
    #[inline(always)]
    #[must_use]
    pub fn gid(&self) -> Gid {
        self.gid
    }

    /// Sets the real group ID of this process.
    #[inline(always)]
    pub fn set_gid(&mut self, gid: Gid) {
        self.gid = gid;
    }

    /// Returns the effective group ID of this process.
    #[inline(always)]
    #[must_use]
    pub fn egid(&self) -> Gid {
        self.egid
    }

    /// Sets the effective group ID of this process.
    #[inline(always)]
    pub fn set_egid(&mut self, egid: Gid) {
        self.egid = egid;
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
    /// the FD. If the FD is equal to or greater than the current soft limit for
    /// `Resource::NOFILE`, returns `Err(body)`.
    pub fn set_fd(&mut self, fd: Fd, body: FdBody) -> Result<Option<FdBody>, FdBody> {
        let limit = self
            .resource_limits
            .get(&Resource::NOFILE)
            .map(|l| l.soft)
            .unwrap_or(INFINITY);

        #[allow(clippy::unnecessary_cast)]
        if limit == INFINITY || (fd.0 as u64) < limit as u64 {
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

    /// Returns the working directory path.
    pub fn getcwd(&self) -> &Path {
        &self.cwd
    }

    /// Changes the working directory.
    ///
    /// This function does not check if the directory exists and is accessible.
    pub fn chdir(&mut self, path: PathBuf) {
        self.cwd = path
    }

    /// Registers a waker that will be woken up when this process resumes.
    ///
    /// The given waker will be woken up when this process is resumed by
    /// [`set_state`](Self::set_state) or [`raise_signal`](Self::raise_signal).
    /// A strong reference to the waker must be held by the caller until the
    /// waker is woken up, when the waker is consumed and the `Cell` content is
    /// set to `None`.
    ///
    /// This function does nothing if the process is not stopped.
    pub fn wake_on_resumption(&mut self, waker: Weak<Cell<Option<Waker>>>) {
        if self.state.is_stopped() {
            self.resumption_awaiters.push(waker);
        }
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
    /// [`state_has_changed`](Self::state_has_changed) flag is set and the
    /// caller must notify the state change by sending `SIGCHLD` to the parent
    /// process.
    #[must_use = "You must send SIGCHLD to the parent if set_state returns true"]
    pub fn set_state(&mut self, state: ProcessState) -> bool {
        let old_state = std::mem::replace(&mut self.state, state);

        if old_state == state {
            false
        } else {
            match state {
                ProcessState::Running => {
                    for weak in self.resumption_awaiters.drain(..) {
                        if let Some(strong) = weak.upgrade() {
                            if let Some(waker) = strong.take() {
                                waker.wake();
                            }
                        }
                    }
                }
                ProcessState::Halted(result) => {
                    if !result.is_stopped() {
                        self.close_fds()
                    }
                }
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
    pub fn blocked_signals(&self) -> &BTreeSet<signal::Number> {
        &self.blocked_signals
    }

    /// Returns the currently pending signals.
    ///
    /// A signal is pending when it has been raised but not yet delivered
    /// because it is being blocked.
    pub fn pending_signals(&self) -> &BTreeSet<signal::Number> {
        &self.pending_signals
    }

    /// Updates the signal blocking mask for this process.
    ///
    /// If this function unblocks a signal, any pending signal is delivered.
    ///
    /// If the signal changes the execution state of the process, this function
    /// returns a `SignalResult` with `process_state_changed` being `true`. In
    /// that case, the caller must send a SIGCHLD to the parent process of this
    /// process.
    #[must_use = "send SIGCHLD if process state has changed"]
    pub fn block_signals(&mut self, how: SigmaskOp, signals: &[signal::Number]) -> SignalResult {
        match how {
            SigmaskOp::Set => self.blocked_signals = signals.iter().copied().collect(),
            SigmaskOp::Add => self.blocked_signals.extend(signals),
            SigmaskOp::Remove => {
                for signal in signals {
                    self.blocked_signals.remove(signal);
                }
            }
        }

        let signals_to_deliver = self.pending_signals.difference(&self.blocked_signals);
        let signals_to_deliver = signals_to_deliver.copied().collect::<Vec<signal::Number>>();
        let mut result = SignalResult::default();
        for signal in signals_to_deliver {
            self.pending_signals.remove(&signal);
            result |= self.deliver_signal(signal);
        }
        result
    }

    /// Returns the current handler for a signal.
    ///
    /// If no handling is set for the signal, the default handling is returned.
    pub fn signal_handling(&self, number: signal::Number) -> SignalHandling {
        let handling = self.signal_handlings.get(&number).copied();
        handling.unwrap_or_default()
    }

    /// Gets and sets the handler for a signal.
    ///
    /// This function sets the handler to `handling` and returns the previous
    /// handler.
    pub fn set_signal_handling(
        &mut self,
        number: signal::Number,
        handling: SignalHandling,
    ) -> SignalHandling {
        let old_handling = self.signal_handlings.insert(number, handling);
        old_handling.unwrap_or_default()
    }

    /// Delivers a signal to this process.
    ///
    /// The action taken on the delivery depends on the current signal handling
    /// for the signal.
    ///
    /// If the signal changes the execution state of the process, this function
    /// returns a `SignalResult` with `process_state_changed` being `true`. In
    /// that case, the caller must send a SIGCHLD to the parent process of this
    /// process.
    #[must_use = "send SIGCHLD if process state has changed"]
    fn deliver_signal(&mut self, signal: signal::Number) -> SignalResult {
        let handling = if signal == signal::SIGKILL || signal == signal::SIGSTOP {
            SignalHandling::Default
        } else {
            self.signal_handling(signal)
        };

        match handling {
            SignalHandling::Default => {
                let name = signal::Name::try_from_raw_virtual(signal.as_raw())
                    .unwrap_or(signal::Name::Sys);
                let process_state_changed = match SignalEffect::of(name) {
                    SignalEffect::None | SignalEffect::Resume => false,
                    SignalEffect::Terminate { core_dump } => {
                        let result = ProcessResult::Signaled { signal, core_dump };
                        self.set_state(ProcessState::Halted(result))
                    }
                    SignalEffect::Suspend => self.set_state(ProcessState::stopped(signal)),
                };
                SignalResult {
                    delivered: true,
                    caught: false,
                    process_state_changed,
                }
            }
            SignalHandling::Ignore => SignalResult {
                delivered: true,
                caught: false,
                process_state_changed: false,
            },
            SignalHandling::Catch => {
                self.caught_signals.push(signal);
                SignalResult {
                    delivered: true,
                    caught: true,
                    process_state_changed: false,
                }
            }
        }
    }

    /// Sends a signal to this process.
    ///
    /// If the signal is being blocked, it will remain pending. Otherwise, it is
    /// immediately delivered.
    ///
    /// If the signal changes the execution state of the process, this function
    /// returns a `SignalResult` with `process_state_changed` being `true`. In
    /// that case, the caller must send a SIGCHLD to the parent process of this
    /// process.
    #[must_use = "send SIGCHLD if process state has changed"]
    pub fn raise_signal(&mut self, signal: signal::Number) -> SignalResult {
        let process_state_changed =
            signal == signal::SIGCONT && self.set_state(ProcessState::Running);

        let mut result = if signal != signal::SIGKILL
            && signal != signal::SIGSTOP
            && self.blocked_signals().contains(&signal)
        {
            self.pending_signals.insert(signal);
            SignalResult::default()
        } else {
            self.deliver_signal(signal)
        };

        result.process_state_changed |= process_state_changed;
        result
    }

    /// Returns the arguments to the last call to
    /// [`execve`](crate::System::execve) on this process.
    #[inline(always)]
    #[must_use]
    pub fn last_exec(&self) -> &Option<(CString, Vec<CString>, Vec<CString>)> {
        &self.last_exec
    }
}

/// Result of operations that may deliver a signal to a process.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct SignalResult {
    /// Whether the signal was delivered to the target process.
    pub delivered: bool,

    /// Whether the delivered signal was caught by a signal handler.
    pub caught: bool,

    /// Whether the signal changed the execution status of the target process.
    ///
    /// This flag is true when the process was terminated, suspended, or resumed.
    pub process_state_changed: bool,
}

impl BitOr for SignalResult {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            delivered: self.delivered | rhs.delivered,
            caught: self.caught | rhs.caught,
            process_state_changed: self.process_state_changed | rhs.process_state_changed,
        }
    }
}

impl BitOrAssign for SignalResult {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::r#virtual::file_system::{FileBody, Inode, Mode};
    use crate::system::r#virtual::io::OpenFileDescription;
    use enumset::EnumSet;
    use futures_util::task::LocalSpawnExt;
    use futures_util::FutureExt;
    use std::collections::VecDeque;
    use std::future::poll_fn;
    use std::rc::Rc;
    use std::task::Poll;

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
        let mut process = Process::with_parent_and_group(Pid(10), Pid(11));

        let file = Rc::new(RefCell::new(Inode {
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
            flags: EnumSet::empty(),
        };
        let writer = FdBody {
            open_file_description: Rc::new(RefCell::new(writer)),
            flags: EnumSet::empty(),
        };

        let reader = process.open_fd(reader).unwrap();
        let writer = process.open_fd(writer).unwrap();
        (process, reader, writer)
    }

    #[test]
    fn process_set_state_wakes_on_resumed() {
        let mut process = Process::with_parent_and_group(Pid(1), Pid(2));
        process.state = ProcessState::stopped(signal::SIGTSTP);
        let process = Rc::new(RefCell::new(process));
        let process2 = Rc::clone(&process);
        let waker = Rc::new(Cell::new(None));
        let task = poll_fn(move |cx| {
            let mut process = process2.borrow_mut();
            if process.state() == ProcessState::Running {
                return Poll::Ready(());
            }
            waker.set(Some(cx.waker().clone()));
            process.wake_on_resumption(Rc::downgrade(&waker));
            Poll::Pending
        });

        let mut executor = futures_executor::LocalPool::new();
        let mut handle = executor.spawner().spawn_local_with_handle(task).unwrap();
        executor.run_until_stalled();
        assert_eq!((&mut handle).now_or_never(), None);

        _ = process.borrow_mut().set_state(ProcessState::Running);
        assert!(executor.try_run_one());
        assert_eq!(handle.now_or_never(), Some(()));
    }

    #[test]
    fn process_set_state_closes_all_fds_on_exit() {
        let (mut process, _reader, _writer) = process_with_pipe();
        assert!(process.set_state(ProcessState::exited(3)));
        assert!(process.fds().is_empty(), "{:?}", process.fds());
    }

    #[test]
    fn process_set_state_closes_all_fds_on_signaled() {
        let (mut process, _reader, _writer) = process_with_pipe();
        assert!(
            process.set_state(ProcessState::Halted(ProcessResult::Signaled {
                signal: signal::SIGINT,
                core_dump: false
            }))
        );
        assert!(process.fds().is_empty(), "{:?}", process.fds());
    }

    #[test]
    fn process_default_signal_blocking_mask() {
        let process = Process::with_parent_and_group(Pid(10), Pid(11));
        let initial_set = process.blocked_signals();
        assert!(initial_set.is_empty(), "{initial_set:?}");
    }

    #[test]
    fn process_sigmask_setmask() {
        let mut process = Process::with_parent_and_group(Pid(10), Pid(11));
        let result = process.block_signals(SigmaskOp::Set, &[signal::SIGINT, signal::SIGCHLD]);
        assert_eq!(result, SignalResult::default());

        let result_set = process.blocked_signals();
        assert!(result_set.contains(&signal::SIGINT));
        assert!(result_set.contains(&signal::SIGCHLD));
        assert_eq!(result_set.len(), 2);

        let result = process.block_signals(SigmaskOp::Set, &[signal::SIGINT, signal::SIGQUIT]);
        assert_eq!(result, SignalResult::default());

        let result_set = process.blocked_signals();
        assert!(result_set.contains(&signal::SIGINT));
        assert!(result_set.contains(&signal::SIGQUIT));
        assert_eq!(result_set.len(), 2);
    }

    #[test]
    fn process_sigmask_block() {
        let mut process = Process::with_parent_and_group(Pid(10), Pid(11));
        let result = process.block_signals(SigmaskOp::Add, &[signal::SIGINT, signal::SIGCHLD]);
        assert_eq!(result, SignalResult::default());

        let result_set = process.blocked_signals();
        assert!(result_set.contains(&signal::SIGINT));
        assert!(result_set.contains(&signal::SIGCHLD));
        assert_eq!(result_set.len(), 2);

        let result = process.block_signals(SigmaskOp::Add, &[signal::SIGINT, signal::SIGQUIT]);
        assert_eq!(result, SignalResult::default());

        let result_set = process.blocked_signals();
        assert!(result_set.contains(&signal::SIGINT));
        assert!(result_set.contains(&signal::SIGQUIT));
        assert!(result_set.contains(&signal::SIGCHLD));
        assert_eq!(result_set.len(), 3);
    }

    #[test]
    fn process_sigmask_unblock() {
        let mut process = Process::with_parent_and_group(Pid(10), Pid(11));
        let result = process.block_signals(SigmaskOp::Add, &[signal::SIGINT, signal::SIGCHLD]);
        assert_eq!(result, SignalResult::default());

        let result = process.block_signals(SigmaskOp::Remove, &[signal::SIGINT, signal::SIGQUIT]);
        assert_eq!(result, SignalResult::default());

        let result_set = process.blocked_signals();
        assert!(result_set.contains(&signal::SIGCHLD));
        assert_eq!(result_set.len(), 1);
    }

    #[test]
    fn process_set_signal_handling() {
        let mut process = Process::with_parent_and_group(Pid(100), Pid(11));
        let old_handling = process.set_signal_handling(signal::SIGINT, SignalHandling::Ignore);
        assert_eq!(old_handling, SignalHandling::Default);
        let old_handling = process.set_signal_handling(signal::SIGTERM, SignalHandling::Catch);
        assert_eq!(old_handling, SignalHandling::Default);

        let old_handling = process.set_signal_handling(signal::SIGINT, SignalHandling::Default);
        assert_eq!(old_handling, SignalHandling::Ignore);
        let old_handling = process.set_signal_handling(signal::SIGTERM, SignalHandling::Ignore);
        assert_eq!(old_handling, SignalHandling::Catch);

        let handling = process.signal_handling(signal::SIGINT);
        assert_eq!(handling, SignalHandling::Default);
        let handling = process.signal_handling(signal::SIGTERM);
        assert_eq!(handling, SignalHandling::Ignore);
        let handling = process.signal_handling(signal::SIGQUIT);
        assert_eq!(handling, SignalHandling::Default);
    }

    #[test]
    fn process_raise_signal_default_nop() {
        let mut process = Process::with_parent_and_group(Pid(42), Pid(11));
        let result = process.raise_signal(signal::SIGCHLD);
        assert_eq!(
            result,
            SignalResult {
                delivered: true,
                caught: false,
                process_state_changed: false,
            }
        );
        assert_eq!(process.state(), ProcessState::Running);
    }

    #[test]
    fn process_raise_signal_default_terminating() {
        let mut process = Process::with_parent_and_group(Pid(42), Pid(11));
        let result = process.raise_signal(signal::SIGTERM);
        assert_eq!(
            result,
            SignalResult {
                delivered: true,
                caught: false,
                process_state_changed: true,
            }
        );
        assert_eq!(
            process.state(),
            ProcessState::Halted(ProcessResult::Signaled {
                signal: signal::SIGTERM,
                core_dump: false
            })
        );
        assert_eq!(process.caught_signals, []);
    }

    #[test]
    fn process_raise_signal_default_aborting() {
        let mut process = Process::with_parent_and_group(Pid(42), Pid(11));
        let result = process.raise_signal(signal::SIGABRT);
        assert_eq!(
            result,
            SignalResult {
                delivered: true,
                caught: false,
                process_state_changed: true,
            }
        );
        assert_eq!(
            process.state(),
            ProcessState::Halted(ProcessResult::Signaled {
                signal: signal::SIGABRT,
                core_dump: true
            })
        );
        assert_eq!(process.caught_signals, []);
        // TODO Check if core dump file has been created
    }

    #[test]
    fn process_raise_signal_default_stopping() {
        let mut process = Process::with_parent_and_group(Pid(42), Pid(11));
        let result = process.raise_signal(signal::SIGTSTP);
        assert_eq!(
            result,
            SignalResult {
                delivered: true,
                caught: false,
                process_state_changed: true,
            }
        );
        assert_eq!(process.state(), ProcessState::stopped(signal::SIGTSTP));
        assert_eq!(process.caught_signals, []);
    }

    #[test]
    fn process_raise_signal_default_continuing() {
        let mut process = Process::with_parent_and_group(Pid(42), Pid(11));
        let _ = process.set_state(ProcessState::stopped(signal::SIGTTOU));
        let result = process.raise_signal(signal::SIGCONT);
        assert_eq!(
            result,
            SignalResult {
                delivered: true,
                caught: false,
                process_state_changed: true,
            }
        );
        assert_eq!(process.state(), ProcessState::Running);
        assert_eq!(process.caught_signals, []);
    }

    #[test]
    fn process_raise_signal_ignored() {
        let mut process = Process::with_parent_and_group(Pid(42), Pid(11));
        process.set_signal_handling(signal::SIGCHLD, SignalHandling::Ignore);
        let result = process.raise_signal(signal::SIGCHLD);
        assert_eq!(
            result,
            SignalResult {
                delivered: true,
                caught: false,
                process_state_changed: false,
            }
        );
        assert_eq!(process.state(), ProcessState::Running);
        assert_eq!(process.caught_signals, []);
    }

    #[test]
    fn process_raise_signal_ignored_and_blocked_sigcont() {
        let mut process = Process::with_parent_and_group(Pid(42), Pid(11));
        let _ = process.set_state(ProcessState::stopped(signal::SIGTTOU));
        let _ = process.set_signal_handling(signal::SIGCONT, SignalHandling::Ignore);
        let _ = process.block_signals(SigmaskOp::Add, &[signal::SIGCONT]);
        let result = process.raise_signal(signal::SIGCONT);
        assert_eq!(
            result,
            SignalResult {
                delivered: false,
                caught: false,
                process_state_changed: true,
            }
        );
        assert_eq!(process.state(), ProcessState::Running);
        assert_eq!(process.caught_signals, []);
        assert!(process.pending_signals.contains(&signal::SIGCONT));
    }

    #[test]
    fn process_raise_signal_caught() {
        let mut process = Process::with_parent_and_group(Pid(42), Pid(11));
        process.set_signal_handling(signal::SIGCHLD, SignalHandling::Catch);
        let result = process.raise_signal(signal::SIGCHLD);
        assert_eq!(
            result,
            SignalResult {
                delivered: true,
                caught: true,
                process_state_changed: false,
            }
        );
        assert_eq!(process.state(), ProcessState::Running);
        assert_eq!(process.caught_signals, [signal::SIGCHLD]);
    }

    #[test]
    fn process_raise_signal_blocked() {
        let mut process = Process::with_parent_and_group(Pid(42), Pid(11));
        process.set_signal_handling(signal::SIGCHLD, SignalHandling::Catch);
        let result = process.block_signals(SigmaskOp::Add, &[signal::SIGCHLD]);
        assert_eq!(
            result,
            SignalResult {
                delivered: false,
                caught: false,
                process_state_changed: false,
            }
        );

        let result = process.raise_signal(signal::SIGCHLD);
        assert_eq!(
            result,
            SignalResult {
                delivered: false,
                caught: false,
                process_state_changed: false,
            }
        );
        assert_eq!(process.state(), ProcessState::Running);
        assert_eq!(process.caught_signals, []);

        let result = process.block_signals(SigmaskOp::Set, &[]);
        assert_eq!(
            result,
            SignalResult {
                delivered: true,
                caught: true,
                process_state_changed: false,
            }
        );
        assert_eq!(process.state(), ProcessState::Running);
        assert_eq!(process.caught_signals, [signal::SIGCHLD]);
    }
}
