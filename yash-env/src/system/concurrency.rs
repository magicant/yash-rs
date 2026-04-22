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

#[cfg(unix)]
use super::real::RealSystem;
use super::{CaughtSignals, Clock, Errno, Fcntl, Read, Result, Select, Write};
use crate::io::Fd;
use crate::waker::{ScheduledWakerQueue, WakerSet};
use futures_util::poll;
use std::cell::{Cell, LazyCell, OnceCell, RefCell};
use std::collections::HashMap;
use std::future::poll_fn;
use std::ops::{Deref, DerefMut};
use std::pin::pin;
use std::rc::{Rc, Weak};
use std::task::Poll::{Pending, Ready};
use std::task::{Context, Waker};
use std::time::{Duration, Instant};

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
/// [`select`](Self::select) method of this struct consolidates blocking
/// behavior into a single system call so that the process can resume execution
/// as soon as any of the specified events occurs.
///
/// For system calls that do not block, such as [`Pipe`], the wrapper directly
/// forwards the call to the inner system without any modification.
///
/// This struct is designed to be used in an [`Rc`] to allow multiple tasks to
/// share the same concurrent system. Some traits, such as [`Read`] and
/// [`Write`], are implemented for `Rc<Concurrent<S>>` instead of
/// `Concurrent<S>` to allow the methods to return futures that capture a clone
/// of the `Rc` and keep it alive until the operation is finished. This is
/// necessary because the futures need to access the internal state of the
/// `Concurrent` system without capturing a reference to the original
/// `Concurrent` struct, which may not live long enough.
///
/// The following example illustrates how multiple concurrent tasks are run in a
/// single-threaded pool:
///
/// ```
/// # use std::rc::Rc;
/// # use yash_env::system::{Concurrent, Pipe as _, Read as _, Write as _};
/// # use yash_env::VirtualSystem;
/// # use futures_util::task::LocalSpawnExt as _;
/// let system = Rc::new(Concurrent::new(VirtualSystem::new()));
/// let system2 = system.clone();
/// let system3 = system.clone();
/// let (reader, writer) = system.pipe().unwrap();
/// let mut executor = futures_executor::LocalPool::new();
///
/// // We add a task that tries to read from the pipe, but nothing has been
/// // written to it, so the task is stalled.
/// let read_task = executor.spawner().spawn_local_with_handle(async move {
///     let mut buffer = [0; 1];
///     system.read(reader, &mut buffer).await.unwrap();
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
/// // stalled. We need to wake it up by calling `select` or `peek`.
/// system3.peek();
///
/// // Now the read task can proceed to the end.
/// let number = executor.run_until(read_task.unwrap());
/// assert_eq!(number, 123);
/// ```
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
    ///
    /// Tasks waiting for signals must retain a strong reference to this list to
    /// see the delivered signals when they are woken up. The list is allocated
    /// when the first task starts waiting for signals and is shared among all
    /// waiting tasks. The list is filled when signals are delivered.
    signals: Weak<SignalList>,
    /// Signal mask for the `select` method
    ///
    /// This cache is initialized from the signal mask the shell inherited from
    /// the parent shell and then updated by [`Concurrent::sigmask`] for use by
    /// `select`. In particular, signals the shell wants to catch are removed
    /// from this mask so they can interrupt `select`. The value is `None` until
    /// the signal mask is first updated by [`Concurrent::sigmask`].
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
            let waker = LazyCell::default();
            loop {
                match this.inner.read(fd, buffer).await {
                    // EWOULDBLOCK is unreachable if it has the same value as EAGAIN.
                    #[allow(unreachable_patterns)]
                    Err(Errno::EAGAIN | Errno::EWOULDBLOCK) => {
                        this.yield_for_read(fd, &waker).await
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
            let waker = LazyCell::default();
            loop {
                match this.inner.write(fd, buffer).await {
                    // EWOULDBLOCK is unreachable if it has the same value as EAGAIN.
                    #[allow(unreachable_patterns)]
                    Err(Errno::EAGAIN | Errno::EWOULDBLOCK) => {
                        this.yield_for_write(fd, &waker).await
                    }

                    result => return result,
                }
            }
        }
    }
}

impl<S> Concurrent<S> {
    async fn yield_for_read<F>(&self, fd: Fd, waker: &LazyCell<Rc<Cell<Option<Waker>>>, F>)
    where
        F: FnOnce() -> Rc<Cell<Option<Waker>>>,
    {
        self.yield_once(fd, waker, |state| &mut state.reads).await
    }

    async fn yield_for_write<F>(&self, fd: Fd, waker: &LazyCell<Rc<Cell<Option<Waker>>>, F>)
    where
        F: FnOnce() -> Rc<Cell<Option<Waker>>>,
    {
        self.yield_once(fd, waker, |state| &mut state.writes).await
    }

    /// Helper method for yielding the current task and registering its waker
    /// for the specified file descriptor and event type (read or write)
    async fn yield_once<F, G>(
        &self,
        fd: Fd,
        waker: &LazyCell<Rc<Cell<Option<Waker>>>, F>,
        target: G,
    ) where
        F: FnOnce() -> Rc<Cell<Option<Waker>>>,
        G: Fn(&mut State) -> &mut HashMap<Fd, WakerSet>,
    {
        let mut first_time = true;
        poll_fn(|context| {
            if first_time {
                first_time = false;
                waker.set(Some(context.waker().clone()));
                target(&mut self.state.borrow_mut())
                    .entry(fd)
                    .or_default()
                    .insert(Rc::downgrade(waker));
                Pending
            } else {
                Ready(())
            }
        })
        .await
    }
}

impl<S> Concurrent<S>
where
    S: Clock,
{
    /// Waits until the specified deadline.
    ///
    /// The returned future will be pending until the specified deadline is
    /// reached, at which point it will complete.
    pub async fn sleep_until(&self, deadline: Instant) {
        let waker: LazyCell<Rc<Cell<Option<Waker>>>> = LazyCell::default();
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

    /// Waits for the specified duration to elapse.
    ///
    /// The returned future will be pending until the specified duration has
    /// elapsed, at which point it will complete.
    pub async fn sleep(&self, duration: Duration) {
        let now = self.inner.now();
        let deadline = now + duration;
        self.sleep_until(deadline).await;
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
        let signals = {
            let mut state = self.state.borrow_mut();
            state.signals.upgrade().unwrap_or_else(|| {
                let signals = Rc::new(SignalList::new());
                state.signals = Rc::downgrade(&signals);
                signals
            })
        };

        let waker: LazyCell<Rc<Cell<Option<Waker>>>> = LazyCell::default();
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
    S: CaughtSignals + Clock + Select,
{
    /// Peeks for any ready events without blocking.
    ///
    /// This method performs a `select` system call with the file descriptors
    /// and timeout of pending tasks, and wakes the tasks whose events are
    /// ready. This method is similar to [`select`](Concurrent::select), but it
    /// does not block and returns immediately.
    pub fn peek(&self) {
        let select = pin!(self.select_impl(true));
        let poll = select.poll(&mut Context::from_waker(Waker::noop()));
        debug_assert_eq!(poll, Ready(()), "peek should not block");
    }

    /// Waits for any of pending tasks to become ready.
    ///
    /// This method performs a `select` system call with the file descriptors
    /// and timeout of pending tasks, and wakes the tasks whose events are
    /// ready. This method should be called in the main loop of the process to
    /// ensure that tasks can make progress. In a typical use case, the main
    /// loop would look like this:
    ///
    /// ```ignore
    /// loop {
    ///     // Run ready tasks until they yield again
    ///     run_ready_tasks();
    ///     // Wait for any pending task to become ready
    ///     concurrent.select().await;
    /// }
    /// ```
    ///
    /// The [`run`](Self::run) method provides a convenient way to implement
    /// such a main loop.
    ///
    /// The future returned by this method will be pending if and only if the
    /// future returned by the inner system's [`select`](Select::select) method
    /// is pending.
    pub async fn select(&self) {
        self.select_impl(false).await;
    }

    #[allow(clippy::await_holding_refcell_ref)]
    async fn select_impl(&self, peek: bool) {
        // In this method, we keep the borrow of `state` across the `await` point. This is
        // intentional because the real `select` call blocks the entire process, so there cannot
        // be any other task that modifies the state while we are waiting for the `select` call to
        // return.
        let mut state = self.state.borrow_mut();

        // Prepare parameters for the `select` call based on the current state
        let mut readers = state.reads.keys().cloned().collect();
        let mut writers = state.writes.keys().cloned().collect();
        let timeout = if peek {
            Some(Duration::ZERO)
        } else {
            state
                .timeouts
                .next_wake_time()
                .map(|target| target.saturating_duration_since(self.inner.now()))
        };
        let signal_mask = (state.signals.strong_count() > 0)
            .then(|| state.select_mask.as_deref())
            .flatten();

        // Perform the `select` call
        let result = self
            .inner
            .select(&mut readers, &mut writers, timeout, signal_mask)
            .await;

        // Wake eligible tasks
        if result != Err(Errno::EINTR) {
            // If `select` succeeded, `readers` and `writers` contain the lists of ready FDs. In
            // case of error, `select` leaves the input lists unmodified (which is required by
            // POSIX), but we don't know which FD caused the error, so we conservatively wake all
            // tasks waiting for any FD.
            wake_tasks_for_ready_fds(&mut state.reads, &readers);
            wake_tasks_for_ready_fds(&mut state.writes, &writers);
        }
        if !state.timeouts.is_empty() {
            state.timeouts.wake(self.inner.now());
        }
        if let Some(signal_list) = state.signals.upgrade() {
            // If the upgrade succeeds, it means there are tasks waiting for signals, so let's
            // check if we have caught any signals.
            let signals = self.inner.caught_signals();
            if !signals.is_empty() {
                let set_result = signal_list.0.set(signals);
                debug_assert_eq!(set_result, Ok(()), "SignalList should not be filled yet");
                state.catches.wake_all();
                // Drop the list so that the next wait_for_signals call can create a new one.
                state.signals = Weak::new();
            }
        }
    }

    /// Runs the given task with concurrency support.
    ///
    /// This function implements the main loop of the shell process. It runs the
    /// given task while also calling [`select`](Self::select) to handle signals
    /// and other events. The task is expected to perform I/O operations using
    /// the methods of this `Concurrent` instance, so that it can yield when the
    /// operations would block. The function returns the output of the task when
    /// it completes.
    ///
    /// The future returned by this method will be pending if and only if the
    /// future returned by the internal system's [`select`](Select::select)
    /// method is pending. For use with [`RealSystem`], the
    /// [`run_sync`](Self::run_sync) method may be more convenient, which works
    /// synchronously.
    pub async fn run<F, T>(&self, task: F) -> T
    where
        F: Future<Output = T>,
    {
        let mut task = pin!(task);
        loop {
            if let Ready(result) = poll!(&mut task) {
                return result;
            }
            self.select().await;
        }
    }
}

fn wake_tasks_for_ready_fds(task_map: &mut HashMap<Fd, WakerSet>, ready_fds: &[Fd]) {
    task_map.retain(|fd, wakers| {
        if ready_fds.contains(fd) {
            wakers.wake_all();
            false
        } else {
            true
        }
    })
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
        // `unwrap` is safe because the list is initialized before being made available to the user.
        self.0.get().unwrap()
    }
}

impl DerefMut for SignalList {
    fn deref_mut(&mut self) -> &mut Vec<crate::signal::Number> {
        // `unwrap` is safe because the list is initialized before being made available to the user.
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
        // `unwrap` is safe because the list is initialized before being made available to the user.
        self.0.into_inner().unwrap()
    }
}

#[cfg(unix)]
impl Concurrent<RealSystem> {
    /// Runs the given task with concurrency support.
    ///
    /// This function implements the main loop of the shell process. It runs the
    /// given task while also calling [`select`](Self::select) to handle signals
    /// and other events. The task is expected to perform I/O operations using
    /// the methods of this `Concurrent` instance, so that it can yield when the
    /// operations would block. The function returns the output of the task when
    /// it completes.
    ///
    /// This method is a specialization of the more general [`run`](Self::run)
    /// method for the case where the inner system is `RealSystem`. Since
    /// [`RealSystem::select`] performs a real `select` system call that blocks
    /// the entire process, this method synchronously returns the output of the
    /// task.
    pub fn run_sync<F, T>(&self, task: F) -> T
    where
        F: Future<Output = T>,
    {
        let future = pin!(self.run(task));
        match future.poll(&mut Context::from_waker(Waker::noop())) {
            Ready(result) => result,
            Pending => unreachable!("`RealSystem::select` should never return `Pending`"),
        }
    }
}

mod delegates;
mod rw_all;
mod signal;

#[cfg(test)]
mod tests {
    use super::super::{
        Close as _, Disposition, Mode, OfdAccess, Open as _, OpenFlag, Pipe as _, SendSignal,
    };
    use super::*;
    use crate::system::r#virtual::{PIPE_SIZE, SIGCHLD, SIGINT, SIGUSR2, VirtualSystem};
    use crate::test_helper::WakeFlag;
    use crate::trap::SignalSystem as _;
    use assert_matches::assert_matches;
    use futures_util::FutureExt as _;
    use std::sync::Arc;
    use std::task::Poll::{Pending, Ready};

    #[test]
    fn peek_with_no_conditions_returns_immediately() {
        let system = Concurrent::new(VirtualSystem::new());
        system.peek();
    }

    #[test]
    fn select_with_no_conditions_never_completes() {
        let system = Concurrent::new(VirtualSystem::new());

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

        let mut buffer1 = [0; 4];
        let mut buffer2 = [0; 4];
        let mut read1 = pin!(system.read(read_fd, &mut buffer1));
        let mut read2 = pin!(system.read(read_fd, &mut buffer2));

        let wake_flag1 = Arc::new(WakeFlag::new());
        let wake_flag2 = Arc::new(WakeFlag::new());
        let waker1 = Waker::from(wake_flag1.clone());
        let waker2 = Waker::from(wake_flag2.clone());
        let mut context1 = Context::from_waker(&waker1);
        let mut context2 = Context::from_waker(&waker2);
        assert_eq!(read1.as_mut().poll(&mut context1), Pending);
        assert_eq!(read2.as_mut().poll(&mut context2), Pending);

        let mut select = pin!(system.select());
        let mut context3 = Context::from_waker(Waker::noop());
        assert_eq!(select.as_mut().poll(&mut context3), Pending);
        assert!(!wake_flag1.is_woken());
        assert!(!wake_flag2.is_woken());

        // Write data to the pipe to make the reads ready
        system
            .write(write_fd, &[1, 2, 3, 4])
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(select.as_mut().poll(&mut context3), Ready(()));
        assert!(wake_flag1.is_woken());
        assert!(wake_flag2.is_woken());
    }

    #[test]
    fn select_wakes_only_read_tasks_with_ready_fd() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let (read_fd1, write_fd1) = system.pipe().unwrap();
        let (read_fd2, _write_fd2) = system.pipe().unwrap();

        let mut buffer1 = [0; 4];
        let mut buffer2 = [0; 4];
        let mut read1 = pin!(system.read(read_fd1, &mut buffer1));
        let mut read2 = pin!(system.read(read_fd2, &mut buffer2));

        let wake_flag1 = Arc::new(WakeFlag::new());
        let wake_flag2 = Arc::new(WakeFlag::new());
        let waker1 = Waker::from(wake_flag1.clone());
        let waker2 = Waker::from(wake_flag2.clone());
        let mut context1 = Context::from_waker(&waker1);
        let mut context2 = Context::from_waker(&waker2);
        assert_eq!(read1.as_mut().poll(&mut context1), Pending);
        assert_eq!(read2.as_mut().poll(&mut context2), Pending);

        let mut select = pin!(system.select());
        let mut context3 = Context::from_waker(Waker::noop());
        assert_eq!(select.as_mut().poll(&mut context3), Pending);
        assert!(!wake_flag1.is_woken());
        assert!(!wake_flag2.is_woken());

        // Write data to the first pipe to make the first read ready
        system
            .write(write_fd1, &[1, 2, 3, 4])
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(select.as_mut().poll(&mut context3), Ready(()));
        assert!(wake_flag1.is_woken());
        assert!(!wake_flag2.is_woken());
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

        let buffer1 = [1, 2, 3, 4];
        let buffer2 = [5, 6, 7, 8];
        let mut write1 = pin!(system.write(write_fd, &buffer1));
        let mut write2 = pin!(system.write(write_fd, &buffer2));

        let wake_flag1 = Arc::new(WakeFlag::new());
        let wake_flag2 = Arc::new(WakeFlag::new());
        let waker1 = Waker::from(wake_flag1.clone());
        let waker2 = Waker::from(wake_flag2.clone());
        let mut context1 = Context::from_waker(&waker1);
        let mut context2 = Context::from_waker(&waker2);
        assert_eq!(write1.as_mut().poll(&mut context1), Pending);
        assert_eq!(write2.as_mut().poll(&mut context2), Pending);

        let mut select = pin!(system.select());
        let mut context3 = Context::from_waker(Waker::noop());
        assert_eq!(select.as_mut().poll(&mut context3), Pending);
        assert!(!wake_flag1.is_woken());
        assert!(!wake_flag2.is_woken());

        // Make space in the pipe buffer to make the writes ready
        let mut read_buffer = [0; PIPE_SIZE];
        system
            .read(read_fd, &mut read_buffer)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(select.as_mut().poll(&mut context3), Ready(()));
        assert!(wake_flag1.is_woken());
        assert!(wake_flag2.is_woken());
    }

    #[test]
    fn select_wakes_only_write_tasks_with_ready_fd() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let (read_fd1, write_fd1) = system.pipe().unwrap();
        let (_read_fd2, write_fd2) = system.pipe().unwrap();
        // Fill the pipe buffers to make the next writes pending
        system
            .write(write_fd1, &[0; PIPE_SIZE])
            .now_or_never()
            .unwrap()
            .unwrap();
        system
            .write(write_fd2, &[0; PIPE_SIZE])
            .now_or_never()
            .unwrap()
            .unwrap();

        let buffer1 = [1, 2, 3, 4];
        let buffer2 = [5, 6, 7, 8];
        let mut write1 = pin!(system.write(write_fd1, &buffer1));
        let mut write2 = pin!(system.write(write_fd2, &buffer2));

        let wake_flag1 = Arc::new(WakeFlag::new());
        let wake_flag2 = Arc::new(WakeFlag::new());
        let waker1 = Waker::from(wake_flag1.clone());
        let waker2 = Waker::from(wake_flag2.clone());
        let mut context1 = Context::from_waker(&waker1);
        let mut context2 = Context::from_waker(&waker2);
        assert_eq!(write1.as_mut().poll(&mut context1), Pending);
        assert_eq!(write2.as_mut().poll(&mut context2), Pending);

        let mut select = pin!(system.select());
        let mut context3 = Context::from_waker(Waker::noop());
        assert_eq!(select.as_mut().poll(&mut context3), Pending);
        assert!(!wake_flag1.is_woken());
        assert!(!wake_flag2.is_woken());

        // Make space in the pipe buffer to make the write ready
        let mut read_buffer = [0; PIPE_SIZE];
        system
            .read(read_fd1, &mut read_buffer)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(select.as_mut().poll(&mut context3), Ready(()));
        assert!(wake_flag1.is_woken());
        assert!(!wake_flag2.is_woken());
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

    #[test]
    fn sleep_completes_after_duration() {
        let system = VirtualSystem::new();
        let state = system.state.clone();
        let now = Instant::now();
        state.borrow_mut().now = Some(now);
        let system = Concurrent::new(system);

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

    #[test]
    fn signal_wait_completes_on_signal() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        system
            .set_disposition(SIGINT, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();
        system
            .set_disposition(SIGCHLD, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();
        system
            .set_disposition(SIGUSR2, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut wait = pin!(system.wait_for_signals());

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_eq!(wait.as_mut().poll(&mut context), Pending);

        let mut select = pin!(system.select());
        let mut null_context = Context::from_waker(Waker::noop());
        assert_eq!(select.as_mut().poll(&mut null_context), Pending);
        assert!(!wake_flag.is_woken());

        // Send signals to make the wait ready
        system.raise(SIGINT).now_or_never().unwrap().unwrap();
        system.raise(SIGCHLD).now_or_never().unwrap().unwrap();
        assert_eq!(select.as_mut().poll(&mut null_context), Ready(()));
        assert!(wake_flag.is_woken());

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_matches!(wait.poll(&mut context), Ready(signals) => {
            assert_matches!(***signals, [SIGINT, SIGCHLD] | [SIGCHLD, SIGINT]);
        });
    }

    #[test]
    fn select_does_not_consume_caught_signals_until_tasks_are_waiting_for_signals() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let (read_fd, write_fd) = system.pipe().unwrap();
        system
            .set_disposition(SIGCHLD, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();
        system.raise(SIGCHLD).now_or_never().unwrap().unwrap();

        let mut buffer = [0; 4];
        let mut read = pin!(system.read(read_fd, &mut buffer));

        let mut null_context = Context::from_waker(Waker::noop());
        assert_eq!(read.as_mut().poll(&mut null_context), Pending);

        system
            .write(write_fd, b"foo")
            .now_or_never()
            .unwrap()
            .unwrap();
        system.select().now_or_never().unwrap();

        let mut wait = pin!(system.wait_for_signals());
        assert_eq!(wait.as_mut().poll(&mut null_context), Pending);

        system.select().now_or_never().unwrap();
        assert_matches!(wait.poll(&mut null_context), Ready(signals) => {
            assert_eq!(**signals, &[SIGCHLD]);
        });
    }

    #[test]
    fn wait_for_signals_can_be_used_many_times() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        system
            .set_disposition(SIGINT, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();
        system
            .set_disposition(SIGCHLD, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut wait1 = pin!(system.wait_for_signals());
        let mut null_context = Context::from_waker(Waker::noop());
        assert_eq!(wait1.as_mut().poll(&mut null_context), Pending);

        system.raise(SIGCHLD).now_or_never().unwrap().unwrap();
        system.select().now_or_never().unwrap();

        let mut wait2 = pin!(system.wait_for_signals());
        assert_eq!(wait2.as_mut().poll(&mut null_context), Pending);

        system.raise(SIGINT).now_or_never().unwrap().unwrap();
        system.select().now_or_never().unwrap();

        assert_matches!(wait1.poll(&mut null_context), Ready(signals) => {
            assert_eq!(**signals, &[SIGCHLD]);
        });
        assert_matches!(wait2.poll(&mut null_context), Ready(signals) => {
            assert_eq!(**signals, &[SIGINT]);
        });
    }

    #[test]
    fn select_completes_when_any_condition_is_ready() {
        let system = VirtualSystem::new();
        let state = system.state.clone();
        let now = Instant::now();
        state.borrow_mut().now = Some(now);
        let system = Rc::new(Concurrent::new(system));
        let (read_fd, write_fd) = system.pipe().unwrap();
        let mut buffer = [0; 4];
        system
            .set_disposition(SIGINT, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut sleep = pin!(system.sleep(Duration::from_secs(3)));
        let mut read = pin!(system.read(read_fd, &mut buffer));
        let mut wait = pin!(system.wait_for_signals());

        let wake_sleep = Arc::new(WakeFlag::new());
        let wake_read = Arc::new(WakeFlag::new());
        let wake_wait = Arc::new(WakeFlag::new());
        let sleep_waker = Waker::from(wake_sleep.clone());
        let read_waker = Waker::from(wake_read.clone());
        let wait_waker = Waker::from(wake_wait.clone());
        let mut sleep_context = Context::from_waker(&sleep_waker);
        let mut read_context = Context::from_waker(&read_waker);
        let mut wait_context = Context::from_waker(&wait_waker);
        assert_eq!(sleep.as_mut().poll(&mut sleep_context), Pending);
        assert_eq!(read.as_mut().poll(&mut read_context), Pending);
        assert_eq!(wait.as_mut().poll(&mut wait_context), Pending);

        let mut select = pin!(system.select());

        let wake_select = Arc::new(WakeFlag::new());
        let select_waker = Waker::from(wake_select.clone());
        let mut select_context = Context::from_waker(&select_waker);
        assert_eq!(select.as_mut().poll(&mut select_context), Pending);
        assert!(!wake_sleep.is_woken());
        assert!(!wake_read.is_woken());
        assert!(!wake_wait.is_woken());
        assert!(!wake_select.is_woken());

        system
            .write(write_fd, b"foo")
            .now_or_never()
            .unwrap()
            .unwrap();
        assert!(wake_select.is_woken());

        let wake_select = Arc::new(WakeFlag::new());
        let select_waker = Waker::from(wake_select.clone());
        let mut select_context = Context::from_waker(&select_waker);
        assert_eq!(select.as_mut().poll(&mut select_context), Ready(()));
        assert!(!wake_sleep.is_woken());
        assert!(wake_read.is_woken());
        assert!(!wake_wait.is_woken());
        assert!(!wake_select.is_woken());

        assert_eq!(read.now_or_never().unwrap(), Ok(3));

        let mut select = pin!(system.select());

        let wake_select = Arc::new(WakeFlag::new());
        let select_waker = Waker::from(wake_select.clone());
        let mut select_context = Context::from_waker(&select_waker);
        assert_eq!(select.as_mut().poll(&mut select_context), Pending);
        assert!(!wake_sleep.is_woken());
        assert!(!wake_wait.is_woken());
        assert!(!wake_select.is_woken());

        state
            .borrow_mut()
            .advance_time(now + Duration::from_secs(3));
        assert!(wake_select.is_woken());

        let wake_select = Arc::new(WakeFlag::new());
        let select_waker = Waker::from(wake_select.clone());
        let mut select_context = Context::from_waker(&select_waker);
        assert_eq!(select.as_mut().poll(&mut select_context), Ready(()));
        assert!(wake_sleep.is_woken());
        assert!(!wake_wait.is_woken());
        assert!(!wake_select.is_woken());

        sleep.now_or_never().unwrap();

        let mut select = pin!(system.select());

        let wake_select = Arc::new(WakeFlag::new());
        let select_waker = Waker::from(wake_select.clone());
        let mut select_context = Context::from_waker(&select_waker);
        assert_eq!(select.as_mut().poll(&mut select_context), Pending);
        assert!(!wake_wait.is_woken());
        assert!(!wake_select.is_woken());

        system.raise(SIGINT).now_or_never().unwrap().unwrap();
        assert!(wake_select.is_woken());

        let wake_select = Arc::new(WakeFlag::new());
        let select_waker = Waker::from(wake_select.clone());
        let mut select_context = Context::from_waker(&select_waker);
        assert_eq!(select.as_mut().poll(&mut select_context), Ready(()));
        assert!(wake_wait.is_woken());
        assert!(!wake_select.is_woken());

        assert_eq!(**wait.now_or_never().unwrap(), &[SIGINT]);
    }

    #[test]
    fn select_wakes_all_reads_and_writes_on_ebadf() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let (read_fd1, _write_fd1) = system.pipe().unwrap();
        let (_read_fd2, write_fd2) = system.pipe().unwrap();
        // Fill the pipe buffer to make the next write pending
        system
            .write(write_fd2, &[0; PIPE_SIZE])
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut read_buffer = [0; 4];
        let mut read = pin!(system.read(read_fd1, &mut read_buffer));
        let mut write = pin!(system.write(write_fd2, &[1, 2, 3, 4]));

        let wake_flag1 = Arc::new(WakeFlag::new());
        let wake_flag2 = Arc::new(WakeFlag::new());
        let waker1 = Waker::from(wake_flag1.clone());
        let waker2 = Waker::from(wake_flag2.clone());
        let mut context1 = Context::from_waker(&waker1);
        let mut context2 = Context::from_waker(&waker2);
        assert_eq!(read.as_mut().poll(&mut context1), Pending);
        assert_eq!(write.as_mut().poll(&mut context2), Pending);

        let mut select = pin!(system.select());

        let wake_select = Arc::new(WakeFlag::new());
        let select_waker = Waker::from(wake_select.clone());
        let mut select_context = Context::from_waker(&select_waker);
        assert_eq!(select.as_mut().poll(&mut select_context), Pending);
        assert!(!wake_flag1.is_woken());
        assert!(!wake_flag2.is_woken());
        assert!(!wake_select.is_woken());

        // Close the file descriptor to make the select call return EBADF
        system.close(read_fd1).unwrap();
        assert!(wake_select.is_woken());

        let wake_select = Arc::new(WakeFlag::new());
        let select_waker = Waker::from(wake_select.clone());
        let mut select_context = Context::from_waker(&select_waker);
        assert_eq!(select.as_mut().poll(&mut select_context), Ready(()));
        assert!(wake_flag1.is_woken());
        assert!(wake_flag2.is_woken());
        assert!(!wake_select.is_woken());
    }

    #[test]
    fn select_does_not_wake_reads_or_writes_on_eintr() {
        // Prepare a system and a pipe
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let (read_fd1, _write_fd1) = system.pipe().unwrap();
        let (_read_fd2, write_fd2) = system.pipe().unwrap();
        // Fill the pipe buffer to make the next write pending
        system
            .write(write_fd2, &[0; PIPE_SIZE])
            .now_or_never()
            .unwrap()
            .unwrap();
        system
            .set_disposition(SIGUSR2, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut read_buffer = [0; 4];
        let mut read = pin!(system.read(read_fd1, &mut read_buffer));
        let mut write = pin!(system.write(write_fd2, &[1]));
        let mut wait = pin!(system.wait_for_signals());

        let wake_flag1 = Arc::new(WakeFlag::new());
        let wake_flag2 = Arc::new(WakeFlag::new());
        let wake_flag3 = Arc::new(WakeFlag::new());
        let waker1 = Waker::from(wake_flag1.clone());
        let waker2 = Waker::from(wake_flag2.clone());
        let waker3 = Waker::from(wake_flag3.clone());
        let mut context1 = Context::from_waker(&waker1);
        let mut context2 = Context::from_waker(&waker2);
        let mut context3 = Context::from_waker(&waker3);
        assert_eq!(read.as_mut().poll(&mut context1), Pending);
        assert_eq!(write.as_mut().poll(&mut context2), Pending);
        assert_eq!(wait.as_mut().poll(&mut context3), Pending);
        assert!(!wake_flag1.is_woken());
        assert!(!wake_flag2.is_woken());
        assert!(!wake_flag3.is_woken());

        system.raise(SIGUSR2).now_or_never().unwrap().unwrap();

        let mut select_fut = pin!(system.select());
        let mut context4 = Context::from_waker(Waker::noop());
        assert_eq!(select_fut.as_mut().poll(&mut context4), Ready(()));
        assert!(!wake_flag1.is_woken());
        assert!(!wake_flag2.is_woken());
    }

    #[test]
    fn signal_wait_is_made_ready_by_peek_after_caught() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        system
            .set_disposition(SIGINT, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();
        system
            .set_disposition(SIGCHLD, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();
        system
            .set_disposition(SIGUSR2, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut wait = pin!(system.wait_for_signals());

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_eq!(wait.as_mut().poll(&mut context), Pending);

        system.peek();
        assert!(!wake_flag.is_woken());

        // Send signals to make the wait ready
        system.raise(SIGINT).now_or_never().unwrap().unwrap();
        system.raise(SIGCHLD).now_or_never().unwrap().unwrap();
        system.peek();
        assert!(wake_flag.is_woken());

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut context = Context::from_waker(&waker);
        assert_matches!(wait.poll(&mut context), Ready(signals) => {
            assert_matches!(***signals, [SIGINT, SIGCHLD] | [SIGCHLD, SIGINT]);
        });
    }
}
