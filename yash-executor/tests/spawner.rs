// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

use futures_task::noop_waker_ref;
use pin_utils::pin_mut;
use std::cell::Cell;
use std::task::Context;
use yash_executor::forwarder::TryReceiveError;
use yash_executor::{Executor, Spawner};

mod spawn_pinned {
    use super::*;

    #[test]
    fn dead_spawner_does_nothing() {
        let run = Cell::new(false);
        let spawner = Spawner::dead();

        let result = unsafe { spawner.spawn_pinned(Box::pin(async { run.set(true) })) };
        assert!(!run.get());

        // Make sure the returned future is the same as the one passed in
        let mut future = result.unwrap_err().0;
        let mut context = Context::from_waker(noop_waker_ref());
        let poll = future.as_mut().poll(&mut context);
        assert!(poll.is_ready());
        assert!(run.get());
    }

    #[test]
    fn spawner_does_nothing_after_executor_was_dropped() {
        let run = Cell::new(false);
        let spawner = Executor::new().spawner();

        let result = unsafe { spawner.spawn_pinned(Box::pin(async { run.set(true) })) };
        assert!(!run.get());

        // Make sure the returned future is the same as the one passed in
        let mut future = result.unwrap_err().0;
        let mut context = Context::from_waker(noop_waker_ref());
        let poll = future.as_mut().poll(&mut context);
        assert!(poll.is_ready());
        assert!(run.get());
    }

    #[test]
    fn spawning_task_outside_task() {
        let run = Cell::new(false);
        let executor = Executor::new();
        let spawner = executor.spawner();

        let result = unsafe { spawner.spawn_pinned(Box::pin(async { run.set(true) })) };
        assert!(result.is_ok());
        assert!(!run.get());
        assert_eq!(executor.wake_count(), 1);

        executor.step();
        assert!(run.get());
    }

    #[test]
    fn spawning_task_inside_task() {
        let executor = Executor::new();
        let spawner = executor.spawner();
        unsafe {
            executor.spawn_pinned(Box::pin(async move {
                let result = spawner.spawn_pinned(Box::pin(async {}));
                assert!(result.is_ok());
            }));
        }

        assert_eq!(executor.run_until_stalled(), 2);
    }
}

mod spawn {
    use super::*;

    #[test]
    fn dead_spawner_does_nothing() {
        let run = Cell::new(false);
        let spawner = Spawner::dead();

        let result = unsafe { spawner.spawn(async { run.set(true) }) };
        assert!(!run.get());

        // Make sure the returned future is the same as the one passed in
        let future = result.unwrap_err().0;
        pin_mut!(future); // TODO std::pin::pin requires Rust 1.68.0
        let mut context = Context::from_waker(noop_waker_ref());
        let poll = future.as_mut().poll(&mut context);
        assert!(poll.is_ready());
        assert!(run.get());
    }

    #[test]
    fn spawner_does_nothing_after_executor_was_dropped() {
        let run = Cell::new(false);
        let spawner = Executor::new().spawner();

        let result = unsafe { spawner.spawn(async { run.set(true) }) };
        assert!(!run.get());

        // Make sure the returned future is the same as the one passed in
        let future = result.unwrap_err().0;
        pin_mut!(future); // TODO std::pin::pin requires Rust 1.68.0
        let mut context = Context::from_waker(noop_waker_ref());
        let poll = future.as_mut().poll(&mut context);
        assert!(poll.is_ready());
        assert!(run.get());
    }

    #[test]
    fn spawning_task_outside_task() {
        let executor = Executor::new();
        let spawner = executor.spawner();

        let receiver = unsafe { spawner.spawn(async { 123 }) }.unwrap();
        assert_eq!(executor.wake_count(), 1);
        assert_eq!(receiver.try_receive(), Err(TryReceiveError::NotSent));

        executor.step();
        assert_eq!(receiver.try_receive(), Ok(123));
    }

    #[test]
    fn spawning_task_inside_task() {
        let executor = Executor::new();
        let spawner = executor.spawner();
        unsafe {
            executor.spawn(async move {
                let result = spawner.spawn(async { 123 }).unwrap().await;
                assert_eq!(result, 123);
            });
        }

        assert_eq!(executor.run_until_stalled(), 2);
    }
}
