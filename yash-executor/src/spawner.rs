// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Implementation of `Spawner`

use crate::forwarder::Receiver;
use crate::{ExecutorState, Spawner};
use alloc::boxed::Box;
use core::future::{Future, IntoFuture};
use core::pin::Pin;

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
    /// If the executor has been dropped, this method will return the future
    /// back to the caller.
    ///
    /// # Safety
    ///
    /// It may be surprising that this method is unsafe. The reason is that the
    /// `Waker` available in the `Context` passed to the future's `poll` method
    /// is thread-unsafe despite `Waker` being `Send` and `Sync`. The `Waker` is
    /// not protected by a lock or atomic operation, and it is your sole
    /// responsibility to ensure that the `Waker` is not passed to or accessed
    /// from other threads.
    pub unsafe fn spawn_pinned(
        &self,
        future: Pin<Box<dyn Future<Output = ()> + 'a>>,
    ) -> Result<(), Pin<Box<dyn Future<Output = ()> + 'a>>> {
        if let Some(state) = self.state.upgrade() {
            ExecutorState::enqueue(&state, future);
            Ok(())
        } else {
            Err(future)
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
    /// # Safety
    ///
    /// See [`spawn_pinned`] for safety considerations.
    ///
    /// [`spawn_pinned`]: Self::spawn_pinned
    pub unsafe fn spawn<F, T>(&self, future: F) -> Result<Receiver<T>, F>
    where
        F: IntoFuture<Output = T> + 'a,
        T: 'a,
    {
        if let Some(state) = self.state.upgrade() {
            Ok(ExecutorState::enqueue_forwarding(&state, future))
        } else {
            Err(future)
        }
    }
}
