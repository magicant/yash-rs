// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
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

//! Items for concurrent task execution

use super::{Clock, Errno, Fcntl, Read, Result, Select, Write};
use crate::io::Fd;
use crate::waker::{ScheduledWakerQueue, WakerSet};
use std::cell::{Cell, LazyCell, OnceCell, RefCell};
use std::collections::HashMap;
use std::future::poll_fn;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::task::Poll::{Pending, Ready};
use std::time::Duration;

/// Decorator for systems that makes blocking I/O operations concurrency-friendly
///
/// This struct is used as a wrapper for systems for enabling concurrent
/// execution of multiple possibly blocking I/O tasks on a single thread. The
/// inner system is expected to implement the [`Read`], [`Write`], and
/// [`Select`] traits with synchronous (blocking) behavior. This struct leaves
/// [`Future`]s returned by I/O methods pending until the I/O operation is ready
/// to avoid blocking the entire process. This allows you to start multiple I/O
/// tasks and wait for them to complete concurrently on a single thread. This
/// struct also provides methods for waiting for signals and waiting for a
/// specified duration, which are represented as [`Future`]s as well. The
/// `select` method of this struct consolidates blocking behavior into a single
/// system call so that the process can resume execution as soon as any of the
/// specified events occurs.
///
/// For system calls that do not block, such as [`Pipe`], the wrapper directly
/// forwards the call to the inner system without any modification.
///
/// [`Pipe`]: super::Pipe
#[derive(Clone, Debug, Default)]
pub struct Concurrent<S> {
    inner: S,
    state: RefCell<State>,
}

/// Internal state for `Concurrent` system
#[derive(Clone, Debug, Default)]
struct State {
    /// Wakers for tasks waiting for read readiness on each file descriptor
    reads: HashMap<Fd, WakerSet>,
    /// Wakers for tasks waiting for write readiness on each file descriptor
    writes: HashMap<Fd, WakerSet>,
    /// Wakers for tasks waiting for a timeout to elapse
    timeouts: ScheduledWakerQueue,

    /// Wakers for tasks waiting for signals to be delivered
    catches: WakerSet,
    /// Shared placeholder for a list of next delivered signals
    signals: Option<Rc<SignalList>>,
    /// Signal mask for the `select` method
    ///
    /// This is the mask the shell inherited from the parent shell minus the
    /// signals the shell wants to catch. The value is `None` until the signal
    /// mask is first updated by [`Concurrent::sigmask`].
    select_mask: Option<Vec<crate::signal::Number>>,
}

impl<S> Concurrent<S> {
    /// Creates a new `Concurrent` system that wraps the given inner system.
    #[must_use]
    pub fn new(inner: S) -> Self {
        let state = Default::default();
        Self { inner, state }
    }
}

/// Reads from a file descriptor concurrently.
///
/// The `read` method internally uses [`Fcntl::get_and_set_nonblocking`] to
/// temporarily set the file descriptor to non-blocking mode while performing
/// the read operation. If the read operation would block (i.e., returns
/// `EAGAIN` or `EWOULDBLOCK`), the method registers the current task's waker in
/// the internal state so that it can be woken up by
/// [`select`](Concurrent::select) when the file descriptor becomes ready for
/// reading.
impl<S> Read for Rc<Concurrent<S>>
where
    S: Fcntl + Read,
{
    fn read<'a>(
        &self,
        fd: Fd,
        buffer: &'a mut [u8],
    ) -> impl Future<Output = Result<usize>> + use<'a, S> {
        let this = Rc::clone(self);
        async move {
            let this = TemporaryNonBlockingGuard::new(&this, fd);
            let waker = LazyCell::new(|| Rc::new(Cell::new(None)));
            loop {
                match this.inner.read(fd, buffer).await {
                    // EWOULDBLOCK is unreachable if it has the same value as EAGAIN.
                    #[allow(unreachable_patterns)]
                    Err(Errno::EAGAIN | Errno::EWOULDBLOCK) => {
                        let mut first_time = true;
                        poll_fn(|context| {
                            if first_time {
                                first_time = false;
                                waker.set(Some(context.waker().clone()));
                                this.state
                                    .borrow_mut()
                                    .reads
                                    .entry(fd)
                                    .or_default()
                                    .insert(Rc::downgrade(&waker));
                                Pending
                            } else {
                                Ready(())
                            }
                        })
                        .await
                    }

                    result => return result,
                }
            }
        }
    }
}

/// Writes to a file descriptor concurrently.
///
/// The `write` method internally uses [`Fcntl::get_and_set_nonblocking`] to
/// temporarily set the file descriptor to non-blocking mode while performing
/// the write operation. If the write operation would block (i.e., returns
/// `EAGAIN` or `EWOULDBLOCK`), the method registers the current task's waker in
/// the internal state so that it can be woken up by
/// [`select`](Concurrent::select) when the file descriptor becomes ready for
/// writing.
impl<S> Write for Rc<Concurrent<S>>
where
    S: Fcntl + Write,
{
    fn write<'a>(
        &self,
        fd: Fd,
        buffer: &'a [u8],
    ) -> impl Future<Output = Result<usize>> + use<'a, S> {
        let this = Rc::clone(self);
        async move {
            let this = TemporaryNonBlockingGuard::new(&this, fd);
            let waker = LazyCell::new(|| Rc::new(Cell::new(None)));
            loop {
                match this.inner.write(fd, buffer).await {
                    // EWOULDBLOCK is unreachable if it has the same value as EAGAIN.
                    #[allow(unreachable_patterns)]
                    Err(Errno::EAGAIN | Errno::EWOULDBLOCK) => {
                        let mut first_time = true;
                        poll_fn(|context| {
                            if first_time {
                                first_time = false;
                                waker.set(Some(context.waker().clone()));
                                this.state
                                    .borrow_mut()
                                    .writes
                                    .entry(fd)
                                    .or_default()
                                    .insert(Rc::downgrade(&waker));
                                Pending
                            } else {
                                Ready(())
                            }
                        })
                        .await
                    }

                    result => return result,
                }
            }
        }
    }
}

impl<S> Concurrent<S>
where
    S: Clock,
{
    /// Waits for the specified duration to elapse.
    ///
    /// The returned future will be pending until the specified duration has
    /// elapsed, at which point it will complete.
    pub async fn sleep(&self, duration: Duration) {
        let now = self.inner.now();
        let deadline = now + duration;
        let waker = LazyCell::new(|| Rc::new(Cell::new(None)));
        poll_fn(|context| {
            if self.inner.now() >= deadline {
                Ready(())
            } else {
                waker.set(Some(context.waker().clone()));
                self.state
                    .borrow_mut()
                    .timeouts
                    .push(deadline, Rc::downgrade(&waker));
                Pending
            }
        })
        .await
    }
}

impl<S> Concurrent<S> {
    /// Waits for signals to be caught.
    ///
    /// The returned future will be pending until any signal is caught, at which
    /// point it will complete with a list of caught signals. The list is shared
    /// among all tasks waiting for signals, so that they can see the same list
    /// of caught signals when they are woken up.
    ///
    /// Before calling this method, the caller needs to [`set_disposition`] for
    /// the signals it wants to catch.
    ///
    /// If this `Concurrent` system is used in an `Env`, you should call
    /// [`Env::wait_for_signals`](crate::Env::wait_for_signals) instead of this
    /// method, so that the trap set can handle the signals properly.
    ///
    /// [`set_disposition`]: crate::trap::SignalSystem::set_disposition
    pub async fn wait_for_signals(&self) -> Rc<SignalList> {
        let signals = self
            .state
            .borrow_mut()
            .signals
            .get_or_insert_with(|| Rc::new(SignalList::new()))
            .clone();

        let waker = LazyCell::new(|| Rc::new(Cell::new(None)));
        poll_fn(|context| {
            if signals.0.get().is_some() {
                Ready(())
            } else {
                waker.set(Some(context.waker().clone()));
                self.state
                    .borrow_mut()
                    .catches
                    .insert(Rc::downgrade(&waker));
                Pending
            }
        })
        .await;

        signals
    }
}

impl<S> Concurrent<S>
where
    S: Clock + Select,
{
    /// Waits for any of pending tasks to become ready.
    ///
    /// TODO
    pub async fn select(&self) {
        let (mut readers, mut writers, timeout, signal_mask) = {
            let state = self.state.borrow();
            let readers = state.reads.keys().cloned().collect();
            let writers = state.writes.keys().cloned().collect();
            let timeout = state
                .timeouts
                .next_wake_time()
                .map(|target| target.saturating_duration_since(self.inner.now()));
            let signal_mask = None; // TODO
            (readers, writers, timeout, signal_mask)
        };

        let _ = self
            .inner
            .select(&mut readers, &mut writers, timeout, signal_mask)
            .await;
        // TODO Handle EBADF

        let mut state = self.state.borrow_mut();
        // TODO wake tasks based on the result of select
        state
            .reads
            .drain()
            .for_each(|(_fd, mut wakers)| wakers.wake_all());
        state
            .writes
            .drain()
            .for_each(|(_fd, mut wakers)| wakers.wake_all());
        if timeout.is_some() {
            state.timeouts.wake(self.inner.now());
        }
    }
}

/// Guard for temporarily setting a file descriptor to non-blocking mode and
/// restoring the original blocking mode when dropped
#[derive(Debug)]
struct TemporaryNonBlockingGuard<'a, S: Fcntl> {
    system: &'a Concurrent<S>,
    fd: Fd,
    original_nonblocking: bool,
}

impl<'a, S: Fcntl> TemporaryNonBlockingGuard<'a, S> {
    fn new(system: &'a Concurrent<S>, fd: Fd) -> Self {
        Self {
            system,
            fd,
            original_nonblocking: system.inner.get_and_set_nonblocking(fd, true) == Ok(true),
        }
    }
}

impl<'a, S: Fcntl> Drop for TemporaryNonBlockingGuard<'a, S> {
    fn drop(&mut self) {
        if !self.original_nonblocking {
            self.system
                .inner
                .get_and_set_nonblocking(self.fd, false)
                .ok();
        }
    }
}

impl<'a, S: Fcntl> Deref for TemporaryNonBlockingGuard<'a, S> {
    type Target = Concurrent<S>;

    fn deref(&self) -> &Self::Target {
        self.system
    }
}

/// List of received signals
///
/// This struct is returned by the [`Concurrent::wait_for_signals`] method to
/// represent the list of signals that have been caught. This is a simple
/// wrapper around `Vec<crate::signal::Number>` that is accessible through
/// `Deref` and `DerefMut`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalList(OnceCell<Vec<crate::signal::Number>>);

impl Deref for SignalList {
    type Target = Vec<crate::signal::Number>;

    fn deref(&self) -> &Vec<crate::signal::Number> {
        // `unwrap` is safe because the list is initialized before being shared with tasks.
        self.0.get().unwrap()
    }
}

impl DerefMut for SignalList {
    fn deref_mut(&mut self) -> &mut Vec<crate::signal::Number> {
        // `unwrap` is safe because the list is initialized before being shared with tasks.
        self.0.get_mut().unwrap()
    }
}

impl SignalList {
    #[must_use]
    fn new() -> Self {
        Self(OnceCell::new())
    }

    /// Consumes the `SignalList` and returns the inner list of signals.
    pub fn into_vec(self) -> Vec<crate::signal::Number> {
        // `unwrap` is safe because the list is initialized before being shared with tasks.
        self.0.into_inner().unwrap()
    }
}

mod delegates;
mod signal;

#[cfg(test)]
mod tests {
    use super::super::{Mode, OfdAccess, Open, OpenFlag, Pipe as _};
    use super::*;
    use crate::system::r#virtual::{PIPE_SIZE, VirtualSystem};
    use crate::test_helper::WakeFlag;
    use futures_util::FutureExt as _;
    use std::pin::pin;
    use std::sync::Arc;
    use std::task::Poll::{Pending, Ready};
    use std::task::{Context, Waker};
    use std::time::Instant;

    #[test]
    fn select_with_no_conditions_never_completes() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));

        let future = pin!(system.select());

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_eq!(future.poll(&mut context), Pending);
        assert!(!wake_flag.is_woken());
    }

    #[test]
    fn regular_file_read_completes_immediately() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let fd = system
            .open(
                c"/foo",
                OfdAccess::ReadOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut buffer = [0; 4];
        let future = pin!(system.read(fd, &mut buffer));

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_eq!(future.poll(&mut context), Ready(Ok(0)));
        assert!(!wake_flag.is_woken());
    }

    #[test]
    fn pipe_read_becomes_ready_on_data_available() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let (read_fd, write_fd) = system.pipe().unwrap();

        let mut buffer = [0; 4];
        let mut read = pin!(system.read(read_fd, &mut buffer));

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_eq!(read.as_mut().poll(&mut context), Pending);

        let mut select = pin!(system.select());
        assert_eq!(select.as_mut().poll(&mut context), Pending);
        assert!(!wake_flag.is_woken());

        // Write data to the pipe to make the read ready
        let write_buffer = [1, 2, 3, 4];
        system
            .write(write_fd, &write_buffer)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(select.as_mut().poll(&mut context), Ready(()));
        assert!(wake_flag.is_woken());

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_eq!(read.poll(&mut context), Ready(Ok(4)));
        assert!(!wake_flag.is_woken());
    }

    #[test]
    fn read_preserves_fd_blocking_mode() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let fd = system
            .open(
                c"/foo",
                OfdAccess::ReadOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut buffer = [0; 4];
        system
            .read(fd, &mut buffer)
            .now_or_never()
            .unwrap()
            .unwrap();

        // The file descriptor should have the same blocking mode as before
        // (which is blocking by default)
        assert_eq!(system.inner.get_and_set_nonblocking(fd, false), Ok(false));

        system.inner.get_and_set_nonblocking(fd, true).ok();
        system
            .read(fd, &mut buffer)
            .now_or_never()
            .unwrap()
            .unwrap();
        // The file descriptor should have the same blocking mode as before
        // (which was set to non-blocking before the read)
        assert_eq!(system.inner.get_and_set_nonblocking(fd, true), Ok(true));
    }

    #[test]
    fn regular_file_write_completes_immediately() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let fd = system
            .open(
                c"/foo",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap()
            .unwrap();

        let buffer = [1, 2, 3, 4];
        let future = pin!(system.write(fd, &buffer));

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_eq!(future.poll(&mut context), Ready(Ok(4)));
        assert!(!wake_flag.is_woken());
    }

    #[test]
    fn pipe_write_becomes_ready_on_buffer_space() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let (read_fd, write_fd) = system.pipe().unwrap();
        // Fill the pipe buffer to make the next write pending
        system
            .write(write_fd, &[0; PIPE_SIZE])
            .now_or_never()
            .unwrap()
            .unwrap();

        let buffer = [1, 2, 3, 4];
        let mut write = pin!(system.write(write_fd, &buffer));

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_eq!(write.as_mut().poll(&mut context), Pending);

        let mut select = pin!(system.select());
        assert_eq!(select.as_mut().poll(&mut context), Pending);
        assert!(!wake_flag.is_woken());

        // Make space in the pipe buffer to make the write ready
        let mut read_buffer = [0; PIPE_SIZE];
        system
            .read(read_fd, &mut read_buffer)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(select.as_mut().poll(&mut context), Ready(()));
        assert!(wake_flag.is_woken());

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_eq!(write.poll(&mut context), Ready(Ok(4)));
        assert!(!wake_flag.is_woken());
    }

    #[test]
    fn write_preserves_fd_blocking_mode() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let fd = system
            .open(
                c"/foo",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap()
            .unwrap();

        let buffer = [1, 2, 3, 4];
        system.write(fd, &buffer).now_or_never().unwrap().unwrap();

        // The file descriptor should have the same blocking mode as before
        // (which is blocking by default)
        assert_eq!(system.inner.get_and_set_nonblocking(fd, false), Ok(false));

        system.inner.get_and_set_nonblocking(fd, true).ok();
        system.write(fd, &buffer).now_or_never().unwrap().unwrap();
        // The file descriptor should have the same blocking mode as before
        // (which was set to non-blocking before the write)
        assert_eq!(system.inner.get_and_set_nonblocking(fd, true), Ok(true));
    }

    // TODO write_all_completes_after_writing_all_data

    #[test]
    fn sleep_completes_after_duration() {
        let system = VirtualSystem::new();
        let state = system.state.clone();
        let now = Instant::now();
        state.borrow_mut().now = Some(now);
        let system = Rc::new(Concurrent::new(system));

        let mut sleep = pin!(system.sleep(Duration::from_secs(1)));

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_eq!(sleep.as_mut().poll(&mut context), Pending);

        let mut select = pin!(system.select());
        assert_eq!(select.as_mut().poll(&mut context), Pending);
        assert!(!wake_flag.is_woken());

        // Advance time by 1 second to make the sleep ready
        state
            .borrow_mut()
            .advance_time(now + Duration::from_secs(1));
        assert_eq!(select.as_mut().poll(&mut context), Ready(()));
        assert!(wake_flag.is_woken());

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_eq!(sleep.poll(&mut context), Ready(()));
        assert!(!wake_flag.is_woken());
    }

    // TODO signal_wait_completes_on_signal
    // TODO select_completes_when_any_condition_is_ready
    // TODO all_tasks_for_same_fd_wake_on_same_select
}
