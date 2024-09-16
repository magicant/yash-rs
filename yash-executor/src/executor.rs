// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Implementation of `Executor`

use crate::Executor;

impl<'a> Executor<'a> {
    /// Creates a new `Executor` with an empty task queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // TODO wake_count

    // TODO spawn_pinned method
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
        todo!()
    }

    /// Runs tasks until there are no more tasks to run.
    ///
    /// This method repeatedly calls `step` until it returns `None`, that is,
    /// there are no more tasks that have been woken up. Returns the number of
    /// completed tasks.
    ///
    /// This method panics if a task is polled recursively.
    pub fn run_until_stalled(&self) -> usize {
        todo!()
    }
}
