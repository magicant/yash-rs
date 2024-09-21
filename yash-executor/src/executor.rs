// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Implementation of `Executor`

use crate::forwarder::{forwarder, Receiver, SendError};
use crate::{Executor, ExecutorState, Spawner, Task};
use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::RefCell;
use core::future::{Future, IntoFuture};
use core::pin::Pin;

impl<'a> Executor<'a> {
    /// Creates a new `Executor` with an empty task queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of tasks that have been woken up but not yet polled.
    #[must_use]
    pub fn wake_count(&self) -> usize {
        self.state.borrow().wake_queue.len()
    }

    /// Adds a task to the task queue.
    ///
    /// The added task is not polled immediately. It will be polled when the
    /// executor runs tasks.
    ///
    /// # Safety
    ///
    /// It may be surprising that this method is unsafe. The reason is that the
    /// `Waker` available in the `Context` passed to the future's `poll` method
    /// is thread-unsafe despite `Waker` being `Send` and `Sync`. The `Waker` is
    /// not protected by a lock or atomic operation, and it is your sole
    /// responsibility to ensure that the `Waker` is not passed to or accessed
    /// from other threads.
    pub unsafe fn spawn_pinned(&self, future: Pin<Box<dyn Future<Output = ()> + 'a>>) {
        ExecutorState::enqueue(&self.state, future);
    }

    /// Adds a task to the task queue.
    ///
    /// This method is an extended version of [`spawn_pinned`] that can take a
    /// non-pinned future and may return a non-unit output. The result of the
    /// future will be sent to the returned receiver.
    ///
    /// The added task is not polled immediately. It will be polled when the
    /// executor runs tasks.
    ///
    /// # Safety
    ///
    /// See [`spawn_pinned`] for safety considerations.
    ///
    /// [`spawn_pinned`]: Self::spawn_pinned
    pub unsafe fn spawn<F, T>(&self, future: F) -> Receiver<T>
    where
        F: IntoFuture<Output = T> + 'a,
        T: 'a,
    {
        ExecutorState::enqueue_forwarding(&self.state, future)
    }

    /// Returns a `Spawner` that can spawn tasks.
    #[must_use]
    pub fn spawner(&self) -> Spawner<'a> {
        let state = Rc::downgrade(&self.state);
        Spawner { state }
    }

    /// Runs a task that has been woken up.
    ///
    /// This method removes a single task from the task queue and polls it.
    /// Returns:
    /// - `Some(true)` if the task is complete
    /// - `Some(false)` if the task is not complete
    /// - `None` if there are no tasks to run
    ///
    /// This method panics if the task is polled recursively.
    pub fn step(&self) -> Option<bool> {
        let task = self.state.borrow_mut().wake_queue.pop_front()?;
        Some(task.poll())
    }

    /// Runs tasks until there are no more tasks to run.
    ///
    /// This method repeatedly calls `step` until it returns `None`, that is,
    /// there are no more tasks that have been woken up. Returns the number of
    /// completed tasks.
    ///
    /// This method panics if a task is polled recursively.
    pub fn run_until_stalled(&self) -> usize {
        let mut completed = 0;
        while let Some(is_complete) = self.step() {
            if is_complete {
                completed += 1;
            }
        }
        completed
    }
}

impl<'a> ExecutorState<'a> {
    pub(crate) fn enqueue(
        this: &Rc<RefCell<Self>>,
        future: Pin<Box<dyn Future<Output = ()> + 'a>>,
    ) {
        let task = Task {
            executor: Rc::downgrade(this),
            future: RefCell::new(Some(future)),
        };
        this.borrow_mut().wake_queue.push_back(Rc::new(task));
    }

    pub(crate) fn enqueue_forwarding<F, T>(this: &Rc<RefCell<Self>>, future: F) -> Receiver<T>
    where
        F: IntoFuture<Output = T> + 'a,
        T: 'a,
    {
        let (sender, receiver) = forwarder();
        let task = Task {
            executor: Rc::downgrade(this),
            future: RefCell::new(Some(Box::pin(async move {
                let send_result = sender.send(future.await);
                debug_assert!(!matches!(send_result, Err((_, SendError::AlreadySent))));
            }))),
        };
        this.borrow_mut().wake_queue.push_back(Rc::new(task));
        receiver
    }
}
