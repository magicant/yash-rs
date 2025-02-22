// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Implementation of `Task`

use crate::Task;
use crate::waker::into_waker;
use alloc::rc::Rc;
use core::task::Context;

impl Task<'_> {
    /// Wakes the task so that it will be polled again by the executor.
    pub fn wake(self: Rc<Self>) {
        let Some(executor) = self.executor.upgrade() else {
            return;
        };

        let wake_queue = &mut executor.borrow_mut().wake_queue;

        // Skip if the task is already enqueued
        if wake_queue.iter().any(|task| Rc::ptr_eq(task, &self)) {
            return;
        }

        wake_queue.push_back(self);
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
        assert_ne!(self.executor.strong_count(), 0, "executor has been dropped");

        let mut future_or_none = self
            .future
            .try_borrow_mut()
            .expect("future polled recursively");
        let Some(future) = future_or_none.as_mut() else {
            return true;
        };

        let waker = into_waker(Rc::clone(self));
        let mut context = Context::from_waker(&waker);
        let poll = future.as_mut().poll(&mut context);
        let is_ready = poll.is_ready();
        if is_ready {
            *future_or_none = None;
        }
        is_ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::rc::Weak;
    use core::cell::{Cell, RefCell};
    use core::future::pending;

    #[test]
    fn waking_without_executor_does_nothing() {
        let task = Rc::new(Task {
            executor: Weak::new(),
            future: RefCell::new(Some(Box::pin(async { unreachable!() }))),
        });
        task.wake();
    }

    #[test]
    fn task_enqueues_itself_when_woken_with_executor() {
        let executor = Rc::default();
        let task = Rc::new(Task {
            executor: Rc::downgrade(&executor),
            future: RefCell::new(Some(Box::pin(async { unreachable!() }))),
        });
        assert_eq!(executor.borrow().wake_queue.len(), 0);

        task.wake();
        assert_eq!(executor.borrow().wake_queue.len(), 1);
    }

    #[test]
    fn task_does_not_enqueue_again_if_already_enqueued() {
        let executor = Rc::default();
        let task = Rc::new(Task {
            executor: Rc::downgrade(&executor),
            future: RefCell::new(Some(Box::pin(async { unreachable!() }))),
        });
        Rc::clone(&task).wake();
        assert_eq!(executor.borrow().wake_queue.len(), 1);

        task.wake();
        assert_eq!(executor.borrow().wake_queue.len(), 1);
    }

    #[test]
    fn multiple_tasks_can_be_enqueued_at_once() {
        let executor = Rc::default();
        let task1 = Rc::new(Task {
            executor: Rc::downgrade(&executor),
            future: RefCell::new(Some(Box::pin(async { unreachable!() }))),
        });
        let task2 = Rc::new(Task {
            executor: Rc::downgrade(&executor),
            future: RefCell::new(Some(Box::pin(async { unreachable!() }))),
        });

        task1.wake();
        task2.wake();
        assert_eq!(executor.borrow().wake_queue.len(), 2);
    }

    #[test]
    #[should_panic = "executor has been dropped"]
    fn polling_without_executor_panics() {
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

    #[test]
    fn polling_pending_future() {
        let executor = Rc::default();
        let task = Rc::new(Task {
            executor: Rc::downgrade(&executor),
            future: RefCell::new(Some(Box::pin(pending()))),
        });
        assert!(!task.poll());
    }

    #[test]
    fn polling_pending_future_again_should_do_nothing() {
        let poll_count = Rc::new(Cell::new(0));
        let executor = Rc::default();
        let task = Rc::new(Task {
            executor: Rc::downgrade(&executor),
            future: RefCell::new(Some(Box::pin(async {
                poll_count.set(poll_count.get() + 1)
            }))),
        });
        assert!(task.poll());
        assert_eq!(poll_count.get(), 1);
        assert!(task.poll());
        assert_eq!(poll_count.get(), 1);
    }

    #[test]
    #[should_panic = "future polled recursively"]
    fn recursive_poll_panics() {
        let executor = Rc::default();
        let task = Rc::new(Task {
            executor: Rc::downgrade(&executor),
            future: RefCell::new(None),
        });
        let task2 = Rc::clone(&task);
        *task.future.borrow_mut() = Some(Box::pin(async move {
            task2.poll();
        }));
        task.poll();
    }
}
