// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Implementation of `Task`

use crate::Task;
use alloc::boxed::Box;

impl Task {
    /// Wakes the task so that it will be polled again by the executor.
    pub fn wake(self: Box<Self>) {
        todo!()
    }

    /// Polls the future contained in the task.
    ///
    /// If the future completes, this method returns `true` and will do
    /// nothing on subsequent calls. If the future is not complete, this
    /// method returns `false`.
    pub fn poll(self) -> bool {
        todo!()
        // TODO Change self.future to None if the future is complete
    }
}
