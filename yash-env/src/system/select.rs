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

//! [`SelectSystem`] and related items

use super::Disposition;
use super::Errno;
use super::Result;
#[cfg(doc)]
use super::SharedSystem;
use super::SigmaskOp;
use super::signal;
use crate::io::Fd;
use crate::system::{CaughtSignals, Clock, Sigaction, Sigmask, Signals};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::binary_heap::PeekMut;
use std::ffi::c_int;
use std::future::Future;
use std::ops::Deref;
use std::ops::DerefMut;
use std::rc::Rc;
use std::rc::Weak;
use std::task::Waker;
use std::time::Duration;
use std::time::Instant;

/// Trait for performing the `select` operation
pub trait Select: Signals {
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
    ///
    /// The return type is a future so that
    /// [virtual systems](crate::system::virtual) can simulate the blocking
    /// behavior of `select` without blocking the entire process. The future
    /// will be ready when one of the above conditions is met. The future may
    /// also return `Pending` if the virtual process is suspended by a signal.
    /// In a [real system](super::real), this function does not work
    /// asynchronously and returns a ready `Future` with the result of the
    /// underlying system call. See the [module-level documentation](super) for
    /// details.
    fn select<'a>(
        &self,
        readers: &'a mut Vec<Fd>,
        writers: &'a mut Vec<Fd>,
        timeout: Option<Duration>,
        signal_mask: Option<&[signal::Number]>,
    ) -> impl Future<Output = Result<c_int>> + use<'a, Self>;
}

/// [System] extended with internal state to support asynchronous functions.
///
/// `SelectSystem` wraps a `System` instance and manages the internal state for
/// asynchronous I/O, signal handling, and timer functions. It coordinates
/// wakers for asynchronous I/O, signals, and timers to call `select` with the
/// appropriate arguments and wake up the wakers when the corresponding events
/// occur.
#[derive(Debug)]
pub struct SelectSystem<S> {
    /// System instance that performs actual system calls
    system: S,
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
    wait_mask: Option<Vec<signal::Number>>,
}

impl<S> Deref for SelectSystem<S> {
    type Target = S;
    fn deref(&self) -> &S {
        &self.system
    }
}

impl<S> DerefMut for SelectSystem<S> {
    fn deref_mut(&mut self) -> &mut S {
        &mut self.system
    }
}

impl<S> SelectSystem<S> {
    /// Creates a new `SelectSystem` that wraps the given `System`.
    pub fn new(system: S) -> Self {
        SelectSystem {
            system,
            io: AsyncIo::new(),
            time: AsyncTime::new(),
            signal: AsyncSignal::new(),
            wait_mask: None,
        }
    }

    /// Updates the actual signal mask and `self.wait_mask`.
    ///
    /// This helper is the async version of the signal mask update. It calls
    /// `system.sigmask` and, after the future succeeds, updates `wait_mask`.
    /// The borrow of `this` is released between each await point.
    ///
    /// This function relies on the fact that the future returned by
    /// `S::sigmask` does not borrow from `&mut self`. This is guaranteed by the
    /// `Sigmask` trait signature, which uses `use<'a, Self>` (not
    /// `use<'_, Self>`), so the future cannot capture the `'_` lifetime of
    /// `&mut self`.
    async fn sigmask_async(
        this: &RefCell<SelectSystem<S>>,
        op: SigmaskOp,
        signal: signal::Number,
    ) -> Result<()>
    where
        S: Sigmask,
    {
        let is_first = this.borrow().wait_mask.is_none();

        if is_first {
            // This is the first call to sigmask. We need to get the current
            // signal mask (which is the mask inherited from the parent shell) and
            // remove the signal from it.
            let mut mask = Vec::new();
            // Note: the borrow_mut() temporary is dropped at the end of this statement.
            let future = this
                .borrow_mut()
                .system
                .sigmask(Some((op, &[signal])), Some(&mut mask));
            // Await after releasing the borrow.
            future.await?;
            // Update wait_mask only on success.
            mask.retain(|&s| s != signal);
            this.borrow_mut().wait_mask = Some(mask);
        } else {
            // We have already called sigmask. We just need to update the mask.
            let future = this
                .borrow_mut()
                .system
                .sigmask(Some((op, &[signal])), None);
            // Await after releasing the borrow.
            future.await?;
            // Update wait_mask only on success.
            let mut borrow = this.borrow_mut();
            borrow.wait_mask.as_mut().unwrap().retain(|&s| s != signal);
        }
        Ok(())
    }

    /// Implements signal disposition query.
    ///
    /// See [`SharedSystem::get_disposition`].
    #[inline]
    pub fn get_disposition(&self, signal: signal::Number) -> Result<Disposition>
    where
        S: Sigaction,
    {
        self.system.get_sigaction(signal)
    }

    /// Implements signal disposition update.
    ///
    /// See [`SharedSystem::set_disposition`].
    pub async fn set_disposition(
        this: &RefCell<SelectSystem<S>>,
        signal: signal::Number,
        handling: Disposition,
    ) -> Result<Disposition>
    where
        S: Sigaction + Sigmask,
    {
        // The order of sigmask and sigaction is important to prevent the signal
        // from being caught. The signal must be caught only when the select
        // function temporarily unblocks the signal. This is to avoid race
        // condition.
        match handling {
            Disposition::Default | Disposition::Ignore => {
                let old_handling = this.borrow_mut().system.sigaction(signal, handling)?;
                Self::sigmask_async(this, SigmaskOp::Remove, signal).await?;
                Ok(old_handling)
            }
            Disposition::Catch => {
                Self::sigmask_async(this, SigmaskOp::Add, signal).await?;
                this.borrow_mut().system.sigaction(signal, handling)
            }
        }
    }

    /// Registers a waker to be woken when the specified file descriptor is ready for reading.
    pub fn add_reader(&mut self, fd: Fd, waker: Weak<RefCell<Option<Waker>>>) {
        self.io.wait_for_reading(fd, waker)
    }

    /// Registers a waker to be woken when the specified file descriptor is ready for writing.
    pub fn add_writer(&mut self, fd: Fd, waker: Weak<RefCell<Option<Waker>>>) {
        self.io.wait_for_writing(fd, waker)
    }

    /// Registers a waker to be woken when the specified time is reached.
    pub fn add_timeout(&mut self, target: Instant, waker: Weak<RefCell<Option<Waker>>>) {
        self.time.push(Timeout { target, waker })
    }

    /// Registers an awaiter for signals.
    ///
    /// This function returns a reference-counted
    /// `SignalStatus::Expected(None)`. The caller must set a waker to the
    /// returned `SignalStatus::Expected`. When signals are caught, the waker is
    /// woken and replaced with `SignalStatus::Caught(signals)`. The caller can
    /// replace the waker in the `SignalStatus::Expected` with another if the
    /// previous waker gets expired and the caller wants to be woken again.
    pub fn add_signal_waker(&mut self) -> Rc<RefCell<SignalStatus>> {
        self.signal.wait_for_signals()
    }

    fn wake_timeouts(&mut self)
    where
        S: Clock,
    {
        if !self.time.is_empty() {
            let now = self.now();
            self.time.wake_if_passed(now);
        }
        self.time.gc();
    }

    fn wake_on_signals(&mut self)
    where
        S: CaughtSignals,
    {
        let signals = self.system.caught_signals();
        if signals.is_empty() {
            self.signal.gc()
        } else {
            self.signal.wake(signals)
        }
    }

    /// Implements the select function for `SharedSystem`.
    ///
    /// See [`SharedSystem::select`].
    ///
    /// This function relies on the fact that the future returned by
    /// `S::select` does not borrow from `&self`. This is guaranteed by the
    /// `Select` trait signature, which uses `use<'a, Self>` (not
    /// `use<'_, Self>`), so the future cannot capture the `'_` lifetime of
    /// `&self`.
    #[allow(clippy::await_holding_refcell_ref)] // false positive
    pub async fn select(this: &RefCell<SelectSystem<S>>, poll: bool) -> Result<()>
    where
        S: Select + CaughtSignals + Clock,
    {
        let me = this.borrow();
        let mut readers = me.io.readers();
        let mut writers = me.io.writers();
        let timeout = if poll {
            Some(Duration::ZERO)
        } else {
            me.time
                .first_target()
                .map(|instant| instant.saturating_duration_since(me.now()))
        };

        let future = me
            .system
            .select(&mut readers, &mut writers, timeout, me.wait_mask.as_deref());

        drop(me);

        // Await after releasing the borrow.
        let inner_result = future.await;

        let mut me = this.borrow_mut();
        let final_result = match inner_result {
            Ok(_) => {
                me.io.wake(&readers, &writers);
                Ok(())
            }
            Err(Errno::EBADF) => {
                // Some of the readers and writers are invalid but we cannot
                // tell which, so we wake up everything.
                me.io.wake_all();
                Err(Errno::EBADF)
            }
            Err(Errno::EINTR) => Ok(()),
            Err(error) => Err(error),
        };
        me.io.gc();
        me.wake_timeouts();
        me.wake_on_signals();
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
    pub fn readers(&self) -> Vec<Fd> {
        self.readers.iter().map(|awaiter| awaiter.fd).collect()
    }

    /// Returns a set of FDs waiting for writing.
    ///
    /// The return value should be passed to the `select` or `pselect` system
    /// call.
    pub fn writers(&self) -> Vec<Fd> {
        self.writers.iter().map(|awaiter| awaiter.fd).collect()
    }

    /// Adds an awaiter for reading.
    pub fn wait_for_reading(&mut self, fd: Fd, waker: Weak<RefCell<Option<Waker>>>) {
        self.readers.push(FdAwaiter { fd, waker });
    }

    /// Adds an awaiter for writing.
    pub fn wait_for_writing(&mut self, fd: Fd, waker: Weak<RefCell<Option<Waker>>>) {
        self.writers.push(FdAwaiter { fd, waker });
    }

    /// Wakes awaiters that are ready for reading/writing.
    ///
    /// FDs in `readers` and `writers` are considered ready and corresponding
    /// awaiters are woken. Once woken, awaiters are removed from `self`.
    pub fn wake(&mut self, readers: &[Fd], writers: &[Fd]) {
        // Dropping awaiters wakes the wakers.
        self.readers
            .retain(|awaiter| !readers.contains(&awaiter.fd));
        self.writers
            .retain(|awaiter| !writers.contains(&awaiter.fd));
    }

    /// Wakes and removes all awaiters.
    pub fn wake_all(&mut self) {
        // Dropping awaiters wakes the wakers.
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
/// `AsyncSignal` is a synchronization primitive for `select`ing on signals.
/// It retains either a list of signals that have been caught or a list of
/// awaiters that are waiting for signals. When both signals and awaiters are
/// present, the awaiters are woken and the signals are passed to the awaiters.
#[derive(Clone, Debug)]
enum AsyncSignal {
    /// No signals have been caught yet, so awaiters are lining up.
    /// The awaiters will be woken when the signal is caught.
    Awaiting(Vec<Weak<RefCell<SignalStatus>>>),
    /// One or more signals have been caught but not yet delivered to any awaiter.
    /// The signals will be passed to the next awaiter.
    Caught(Vec<signal::Number>),
}

/// Status of awaited signals
#[derive(Clone, Debug)]
pub enum SignalStatus {
    /// No signal has been caught.
    /// The waker will be woken when the signal is caught.
    Expected(Option<Waker>),

    /// One or more signals have been caught.
    /// The slice contains the caught signals.
    Caught(Rc<[signal::Number]>),
}

impl AsyncSignal {
    /// Returns a new empty `AsyncSignal`.
    pub fn new() -> Self {
        Self::Awaiting(Vec::new())
    }

    /// Discards awaiters that are no longer valid.
    ///
    /// This function removes [`SignalStatus`]es that are not retained with any
    /// strong references. Such `SignalStatus`es will never wake any task, so it
    /// does not make sense to keep them in memory.
    pub fn gc(&mut self) {
        match self {
            Self::Awaiting(awaiters) => awaiters.retain(|awaiter| awaiter.strong_count() > 0),
            Self::Caught(_) => {}
        }
    }

    /// Adds an awaiter for signals.
    ///
    /// If any signal has already been caught, this function returns
    /// `SignalStatus::Caught(signals)`.
    ///
    /// Otherwise, this function returns a reference-counted
    /// `SignalStatus::Expected(None)`. The caller should set a waker to the
    /// returned `SignalStatus::Expected` and retain the shared status. When a
    /// signal is caught later ([`wake`](Self::wake)), the waker set to
    /// `SignalStatus::Expected` is woken and the status is replaced with
    /// `SignalStatus::Caught(signals)`.
    ///
    /// The caller can replace the waker in the `SignalStatus::Expected` with
    /// another if the previous waker gets expired and the caller wants to be
    /// woken again.
    pub fn wait_for_signals(&mut self) -> Rc<RefCell<SignalStatus>> {
        match std::mem::replace(self, AsyncSignal::Awaiting(Vec::new())) {
            AsyncSignal::Awaiting(mut awaiters) => {
                let status = Rc::new(RefCell::new(SignalStatus::Expected(None)));
                awaiters.push(Rc::downgrade(&status));
                *self = AsyncSignal::Awaiting(awaiters);
                status
            }

            AsyncSignal::Caught(signals) => {
                debug_assert!(!signals.is_empty());
                Rc::new(RefCell::new(SignalStatus::Caught(signals.into())))
            }
        }
    }

    /// Passes caught signals to awaiters.
    ///
    /// This function wakes up all wakers in pending `SignalStatus`es and
    /// removes them from `self`.
    ///
    /// This function borrows `SignalStatus`es returned from
    /// [`wait_for_signals`](Self::wait_for_signals) so you must not have
    /// conflicting borrows.
    ///
    /// If there is no pending awaiters, that is, `wait_for_signals` has not
    /// been called, then this function retains the given signals so that they
    /// can be immediately returned next time `wait_for_signals` is called.
    pub fn wake(&mut self, signals: Vec<signal::Number>) {
        if signals.is_empty() {
            return;
        }

        match self {
            AsyncSignal::Caught(accumulated_signals) => accumulated_signals.extend(signals),

            AsyncSignal::Awaiting(awaiters) => {
                enum Woke {
                    None(Vec<signal::Number>),
                    Some(Rc<[signal::Number]>),
                }
                let mut woke = Woke::None(signals);

                for status in awaiters.drain(..) {
                    let Some(status) = status.upgrade() else {
                        continue;
                    };

                    let signals = match woke {
                        Woke::None(signals) => Rc::from(signals),
                        Woke::Some(signals) => signals,
                    };
                    woke = Woke::Some(Rc::clone(&signals));

                    let mut status_ref = status.borrow_mut();
                    let new_status = SignalStatus::Caught(signals);
                    let old_status = std::mem::replace(&mut *status_ref, new_status);
                    drop(status_ref);
                    if let SignalStatus::Expected(Some(waker)) = old_status {
                        waker.wake();
                    }
                }

                if let Woke::None(signals) = woke {
                    // retain the signals for a next awaiter
                    *self = AsyncSignal::Caught(signals);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::r#virtual::{SIGCHLD, SIGINT, SIGUSR1};
    use super::*;
    use crate::test_helper::WakeFlag;
    use assert_matches::assert_matches;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    #[test]
    fn async_io_has_no_default_readers_or_writers() {
        let async_io = AsyncIo::new();
        assert_eq!(async_io.readers(), []);
        assert_eq!(async_io.writers(), []);
    }

    #[test]
    fn async_io_non_empty_readers_and_writers() {
        let mut async_io = AsyncIo::new();
        let waker = Rc::new(RefCell::new(Some(Waker::noop().clone())));
        async_io.wait_for_reading(Fd::STDIN, Rc::downgrade(&waker));
        async_io.wait_for_writing(Fd::STDOUT, Rc::downgrade(&waker));
        async_io.wait_for_writing(Fd::STDERR, Rc::downgrade(&waker));

        assert_eq!(async_io.readers(), [Fd::STDIN]);
        let mut writers = async_io.writers();
        writers.sort();
        assert_eq!(writers, [Fd::STDOUT, Fd::STDERR]);
    }

    #[test]
    fn async_io_wake() {
        let mut async_io = AsyncIo::new();
        let waker = Rc::new(RefCell::new(Some(Waker::noop().clone())));
        async_io.wait_for_reading(Fd(3), Rc::downgrade(&waker));
        async_io.wait_for_reading(Fd(4), Rc::downgrade(&waker));
        async_io.wait_for_writing(Fd(4), Rc::downgrade(&waker));
        async_io.wake(&[Fd(4)], &[Fd(4)]);

        assert_eq!(async_io.readers(), [Fd(3)]);
        assert_eq!(async_io.writers(), []);
    }

    #[test]
    fn async_io_wake_all() {
        let mut async_io = AsyncIo::new();
        let waker = Rc::new(RefCell::new(Some(Waker::noop().clone())));
        async_io.wait_for_reading(Fd::STDIN, Rc::downgrade(&waker));
        async_io.wait_for_writing(Fd::STDOUT, Rc::downgrade(&waker));
        async_io.wait_for_writing(Fd::STDERR, Rc::downgrade(&waker));
        async_io.wake_all();
        assert_eq!(async_io.readers(), []);
        assert_eq!(async_io.writers(), []);
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
        let waker = Rc::new(RefCell::new(Some(Waker::noop().clone())));
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
    fn async_signal_wait_and_wake() {
        let mut async_signal = AsyncSignal::new();
        let status_1 = async_signal.wait_for_signals();
        let status_2 = async_signal.wait_for_signals();
        let wake_flag_1 = Arc::new(WakeFlag::new());
        let wake_flag_2 = Arc::new(WakeFlag::new());
        assert_matches!(&mut *status_1.borrow_mut(), SignalStatus::Expected(waker) => {
            assert!(waker.is_none());
            *waker = Some(wake_flag_1.clone().into());
        });
        assert_matches!(&mut *status_2.borrow_mut(), SignalStatus::Expected(waker) => {
            assert!(waker.is_none());
            *waker = Some(wake_flag_2.clone().into());
        });

        async_signal.wake(vec![SIGCHLD, SIGUSR1]);

        assert!(wake_flag_1.0.load(Ordering::Relaxed));
        assert!(wake_flag_2.0.load(Ordering::Relaxed));
        assert_matches!(&*status_1.borrow(), SignalStatus::Caught(signals) => {
            assert_eq!(**signals, [SIGCHLD, SIGUSR1]);
        });
        assert_matches!(&*status_2.borrow(), SignalStatus::Caught(signals) => {
            assert_eq!(**signals, [SIGCHLD, SIGUSR1]);
        });
    }

    #[test]
    fn async_signal_wake_and_wait() {
        let mut async_signal = AsyncSignal::new();
        async_signal.wake(vec![SIGINT, SIGCHLD]);

        let status = async_signal.wait_for_signals();

        assert_matches!(&*status.borrow(), SignalStatus::Caught(signals) => {
            assert_eq!(**signals, [SIGINT, SIGCHLD]);
        });
    }

    #[test]
    fn async_signal_wake_twice_and_wait() {
        let mut async_signal = AsyncSignal::new();
        async_signal.wake(vec![SIGINT]);
        async_signal.wake(vec![SIGUSR1]);

        let status = async_signal.wait_for_signals();

        assert_matches!(&*status.borrow(), SignalStatus::Caught(signals) => {
            assert_eq!(**signals, [SIGINT, SIGUSR1]);
        });
    }

    #[test]
    fn async_signal_empty_wake() {
        let mut async_signal = AsyncSignal::new();
        let status = async_signal.wait_for_signals();
        let wake_flag = Arc::new(WakeFlag::new());
        assert_matches!(&mut *status.borrow_mut(), SignalStatus::Expected(waker) => {
            assert!(waker.is_none());
            *waker = Some(wake_flag.clone().into());
        });

        async_signal.wake(vec![]);

        assert!(!wake_flag.is_woken());
        // to assert that the waker is not modified, we wake the waker ourself
        assert_matches!(&*status.borrow(), SignalStatus::Expected(Some(waker)) => {
            waker.wake_by_ref();
        });
        assert!(wake_flag.is_woken());
    }

    #[test]
    fn async_signal_phantom_wake() {
        // In this test case, we drop the `SignalStatus` returned from the first
        // `wait_for_signals` call before calling `wake`. `AsyncSignal` should
        // retain the signals and return them to the next `wait_for_signals`
        // call.
        let mut async_signal = AsyncSignal::new();
        let status_1 = async_signal.wait_for_signals();
        drop(status_1);

        async_signal.wake(vec![SIGINT]);

        let status_2 = async_signal.wait_for_signals();
        assert_matches!(&*status_2.borrow(), SignalStatus::Caught(signals) => {
            assert_eq!(**signals, [SIGINT]);
        });
    }
}
