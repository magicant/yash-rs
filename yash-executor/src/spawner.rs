// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Implementation of `Spawner`

use crate::forwarder::Receiver;
use crate::{ExecutorState, Spawner};
use alloc::boxed::Box;
use core::fmt::Debug;
use core::future::{Future, IntoFuture};
use core::pin::Pin;

/// Error returned when a task cannot be spawned
///
/// This error is returned from [`Spawner`]'s methods when the executor has been
/// dropped and the task cannot be spawned. The error contains the task that
/// could not be spawned, allowing the caller to reuse the task.
///
/// `SpawnError` implements `Debug` for all `F` regardless of whether `F` does.
/// This allows the use of `unwrap` and `expect` on `Result<_, SpawnError<F>>`.
#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SpawnError<F>(pub F);

impl<F> Debug for SpawnError<F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        "SpawnError(_)".fmt(f)
    }
}
// TODO Specialize Debug for F when F: Debug

impl<'a> Spawner<'a> {
    /// Creates a dummy `Spawner` that is not associated with any executor and
    /// thus cannot spawn tasks.
    #[must_use]
    pub fn dead() -> Self {
        Self {
            state: Default::default(),
        }
    }

    /// Adds the given future to the executor's task queue so that it will be
    /// polled when the executor is run.
    ///
    /// The added task is not polled immediately. It will be polled when the
    /// executor runs tasks.
    ///
    /// If the executor has been dropped, this method will return the future
    /// wrapped in a `SpawnError`. The caller can then reuse the future with
    /// another executor or handle the error in some other way.
    ///
    /// # Safety
    ///
    /// It may be surprising that this method is unsafe. The reason is that the
    /// `Waker` available in the `Context` passed to the future's `poll` method
    /// is thread-unsafe despite `Waker` being `Send` and `Sync`. The `Waker` is
    /// not protected by a lock or atomic operation, and it is your sole
    /// responsibility to ensure that the `Waker` is not passed to or accessed
    /// from other threads.
    #[allow(clippy::type_complexity)]
    pub unsafe fn spawn_pinned(
        &self,
        future: Pin<Box<dyn Future<Output = ()> + 'a>>,
    ) -> Result<(), SpawnError<Pin<Box<dyn Future<Output = ()> + 'a>>>> {
        if let Some(state) = self.state.upgrade() {
            ExecutorState::enqueue(&state, future);
            Ok(())
        } else {
            Err(SpawnError(future))
        }
    }

    /// Adds the given future to the executor's task queue so that it will be
    /// polled when the executor is run.
    ///
    /// This method is an extended version of [`spawn_pinned`] that can take a
    /// non-pinned future and may return a non-unit output. The result of the
    /// future will be sent to the returned receiver.
    ///
    /// The added task is not polled immediately. It will be polled when the
    /// executor runs tasks.
    ///
    /// If the executor has been dropped, this method will return the future
    /// wrapped in a `SpawnError`. The caller can then reuse the future with
    /// another executor or handle the error in some other way.
    ///
    /// # Safety
    ///
    /// See [`spawn_pinned`] for safety considerations.
    ///
    /// [`spawn_pinned`]: Self::spawn_pinned
    pub unsafe fn spawn<F, T>(&self, future: F) -> Result<Receiver<T>, SpawnError<F>>
    where
        F: IntoFuture<Output = T> + 'a,
        T: 'a,
    {
        if let Some(state) = self.state.upgrade() {
            Ok(ExecutorState::enqueue_forwarding(&state, future))
        } else {
            Err(SpawnError(future))
        }
    }
}
