// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! `yash-executor` is a library for running concurrent tasks in a
//! single-threaded context. This crate supports `no_std` configurations but
//! requires the `alloc` crate.
//!
//! The [`Executor`] provided by this crate can be instantiated more than once
//! to run multiple sets of tasks concurrently. Each executor maintains its
//! own set of tasks and does not share tasks with other executors. This is
//! different from other executor implementations that use a global or
//! thread-local executor.
//!
//! This crate is free of locks and atomic operations at the cost of
//! [unsafe spawning](Executor::spawn_pinned). Wakers used in this crate are
//! thread-unsafe and not guarded by locks or atomics, so you must ensure that
//! wakers are not shared between threads.
//!
//! ```
//! # use yash_executor::Executor;
//! # use yash_executor::forwarder::TryReceiveError;
//! let executor = Executor::new();
//!
//! // Spawn a task that returns 42
//! let receiver = unsafe { executor.spawn(async { 42 }) };
//!
//! // The task is not yet complete
//! assert_eq!(receiver.try_receive(), Err(TryReceiveError::NotSent));
//!
//! // Run the executor until the task is complete
//! executor.run_until_stalled();
//!
//! // Now we have the result
//! assert_eq!(receiver.try_receive(), Ok(42));
//! ```
//!
//! [`Spawner`]s provide a subset of the functionality of [`Executor`] to allow
//! spawning tasks without access to the full executor. It is useful for adding
//! tasks from within another task without creating cyclic
//!
//! The [`forwarder`] module provides utilities for forwarding the result of a
//! future to another future. The [`forwarder`](forwarder::forwarder) function
//! creates a pair of [`Sender`] and [`Receiver`] that share an internal state
//! to communicate the result of a future. A `Receiver` is also returned from
//! the [`Executor::spawn`] method to receive the result of a future.
//!
//! [`Sender`]: forwarder::Sender
//! [`Receiver`]: forwarder::Receiver

#![no_std]
extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::rc::{Rc, Weak};
use core::cell::RefCell;
use core::fmt::Debug;
use core::future::Future;
use core::pin::Pin;

/// Interface for running concurrent tasks
///
/// You call the [`spawn_pinned`](Self::spawn_pinned) or [`spawn`](Self::spawn)
/// method to add a task to the executor. Just adding a task to the executor
/// does not run it. You need to call the [`step`](Self::step) or
/// [`run_until_stalled`](Self::run_until_stalled) method to run the tasks.
///
/// `Executor` implements `Clone` but all clones share the same set of tasks.
/// Separately created `Executor` instances do not share tasks.
#[derive(Clone, Debug, Default)]
pub struct Executor<'a> {
    state: Rc<RefCell<ExecutorState<'a>>>,
}

/// Interface for spawning tasks
///
/// `Spawner` provides a subset of the functionality of `Executor` to allow
/// spawning tasks without access to the full executor.
///
/// `Spawner` instances can be cloned and share the same executor state.
/// `Spawner`s maintain a weak reference to the executor state, so they do not
/// prevent the executor from being dropped. If the executor is dropped, the
/// `Spawner` will not be able to spawn any more tasks.
///
/// To obtain a `Spawner` from an `Executor`, use the [`Executor::spawner`]
/// method. The [`dead`](Self::dead) and `default` functions return a `Spawner`
/// that can never spawn tasks.
///
/// ```
/// # use yash_executor::Executor;
/// let executor = Executor::new();
/// let spawner = executor.spawner();
/// let final_receiver = unsafe {
///     executor.spawn(async move {
///         let receiver_1 = spawner.spawn(async { 1 }).unwrap();
///         let receiver_2 = spawner.spawn(async { 2 }).unwrap();
///         receiver_2.await + receiver_1.await
///     })
/// };
/// executor.run_until_stalled();
/// assert_eq!(final_receiver.try_receive(), Ok(3));
/// ```
#[derive(Clone, Debug, Default)]
pub struct Spawner<'a> {
    state: Weak<RefCell<ExecutorState<'a>>>,
}

/// Internal state of the executor
#[derive(Default)]
struct ExecutorState<'a> {
    /// Queue of woken tasks to be executed
    ///
    /// Tasks are added to the queue when they are woken up by another task or
    /// when they are spawned. The executor removes a task from the queue and
    /// polls it once. If the poll method returns `Poll::Pending`, the task
    /// needs to be added back to the queue by some waker when it is ready to
    /// be polled again.
    wake_queue: VecDeque<Rc<Task<'a>>>,
    // We don't need to store tasks that are waiting to be woken up because they
    // are retained by wakers. This also prevents leaking tasks that are never
    // woken up.
}

impl Debug for ExecutorState<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExecutorState")
            .field(
                "wake_queue",
                &format_args!("(len = {})", self.wake_queue.len()),
            )
            .finish()
    }
}

/// State of a task to be executed
struct Task<'a> {
    /// Shared state of the executor for running this task
    executor: Weak<RefCell<ExecutorState<'a>>>,

    /// The task to be executed
    ///
    /// This value becomes `None` when the task is completed to prevent polling
    /// it again.
    future: RefCell<Option<Pin<Box<dyn Future<Output = ()> + 'a>>>>,
}

pub mod forwarder;

mod executor;
mod spawner;
mod task;
mod waker;

pub use spawner::SpawnError;
