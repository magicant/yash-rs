// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

use yash_executor::Executor;

mod spawn_pinned {
    use super::*;

    #[test]
    fn increases_wake_count() {
        let executor = Executor::new();
        assert_eq!(executor.wake_count(), 0);

        unsafe { executor.spawn_pinned(Box::pin(async {})) };
        assert_eq!(executor.wake_count(), 1);
    }

    #[test]
    fn does_not_poll_added_future() {
        let executor = Executor::new();
        unsafe { executor.spawn_pinned(Box::pin(async { unreachable!() })) };
    }
}

mod step {
    use super::*;
    use std::cell::Cell;
    use std::future::{pending, poll_fn};
    use std::task::Poll;

    #[test]
    fn returns_none_when_no_tasks() {
        let executor = Executor::new();
        assert_eq!(executor.step(), None);
    }

    #[test]
    fn returns_false_when_task_not_complete() {
        let executor = Executor::new();
        unsafe { executor.spawn_pinned(Box::pin(pending())) };
        assert_eq!(executor.step(), Some(false));
    }

    #[test]
    fn returns_true_when_task_complete() {
        let executor = Executor::new();
        unsafe { executor.spawn_pinned(Box::pin(async {})) };
        assert_eq!(executor.step(), Some(true));
    }

    #[test]
    fn removes_task_from_wake_queue() {
        let executor = Executor::new();
        unsafe { executor.spawn_pinned(Box::pin(async {})) };
        executor.step();
        assert_eq!(executor.wake_count(), 0);
    }

    #[test]
    fn supports_yielding_future() {
        let poll_count = Cell::new(0);
        let executor = Executor::new();
        unsafe {
            executor.spawn_pinned(Box::pin(poll_fn(|cx| match poll_count.get() {
                0 => {
                    poll_count.set(1);
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                1 => {
                    poll_count.set(2);
                    Poll::Ready(())
                }
                _ => unreachable!(),
            })))
        };

        executor.step();
        assert_eq!(poll_count.get(), 1);

        executor.step();
        assert_eq!(poll_count.get(), 2);
    }

    #[test]
    fn supports_spawning_tasks_within_tasks() {
        let executor1 = Executor::new();
        let executor2 = executor1.clone();
        unsafe {
            executor1.spawn_pinned(Box::pin(async move {
                executor2.spawn_pinned(Box::pin(async {}));
            }))
        };

        executor1.step();
        assert_eq!(executor1.wake_count(), 1);
        executor1.step();
        assert_eq!(executor1.wake_count(), 0);
    }
}

mod run_until_stalled {
    use super::*;
    use std::future::pending;

    #[test]
    fn returns_zero_when_no_tasks() {
        let executor = Executor::new();
        assert_eq!(executor.run_until_stalled(), 0);
    }

    #[test]
    fn returns_number_of_completed_tasks() {
        let executor1 = Executor::new();
        let executor2 = executor1.clone();
        unsafe {
            executor1.spawn_pinned(Box::pin(async move {
                executor2.spawn_pinned(Box::pin(async {}));
            }));
            executor1.spawn_pinned(Box::pin(pending()));
        }

        assert_eq!(executor1.run_until_stalled(), 2);
        assert_eq!(executor1.wake_count(), 0);
    }
}
