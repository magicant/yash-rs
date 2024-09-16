// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Implementation of `Task`

use crate::Task;
use alloc::rc::Rc;
use core::task::{Context, Waker};

impl Task<'_> {
    /// Wakes the task so that it will be polled again by the executor.
    pub fn wake(self: Rc<Self>) {
        todo!()
    }

    /// Polls the future contained in the task.
    ///
    /// If the future completes, this method returns `true` and will do
    /// nothing on subsequent calls. If the future is not complete, this
    /// method returns `false`.
    ///
    /// If `self.executor` has been dropped or the task is polled recursively,
    /// this method panics.
    pub fn poll(self: &Rc<Self>) -> bool {
        let mut future = self.future.borrow_mut();
        match future.as_mut() {
            None => todo!(),
            Some(future) => {
                if self.executor.strong_count() == 0 {
                    todo!("executor has been dropped");
                }
                let waker = futures_task::noop_waker();
                let mut context = Context::from_waker(&waker);
                let poll = future.as_mut().poll(&mut context);
                true // TODO false if poll is not ready
            }
        }

        // TODO Change self.future to None if the future is complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::rc::Weak;
    use core::cell::{Cell, RefCell};

    #[test]
    #[should_panic = "executor has been dropped"]
    fn polling_without_executor() {
        let task = Rc::new(Task {
            executor: Weak::new(),
            future: RefCell::new(Some(Box::pin(async { unreachable!() }))),
        });
        task.poll();
    }

    #[test]
    fn polling_ready_future() {
        let polled = Rc::new(Cell::new(false));
        let executor = Rc::default();
        let task = Rc::new(Task {
            executor: Rc::downgrade(&executor),
            future: RefCell::new(Some(Box::pin(async { polled.set(true) }))),
        });
        assert!(task.poll());
        assert!(polled.get());
    }
}
