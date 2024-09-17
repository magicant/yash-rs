// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Implementation of `Executor`

use crate::{Executor, Task};
use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::RefCell;
use core::future::Future;
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
    /// TODO: This method should be unsafe
    pub fn spawn_pinned(&self, future: Pin<Box<dyn Future<Output = ()> + 'a>>) {
        let task = Task {
            executor: Rc::downgrade(&self.state),
            future: RefCell::new(Some(future)),
        };
        self.state.borrow_mut().wake_queue.push_back(Rc::new(task));
    }

    // TODO spawn method that takes a non-pinned future that may return a non-unit output

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
        0 // TODO
    }
}
