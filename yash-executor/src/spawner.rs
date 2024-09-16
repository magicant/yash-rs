// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Implementation of `Spawner`

use crate::{ExecutorState, Spawner};
use alloc::boxed::Box;
use alloc::rc::Weak;
use core::cell::RefCell;
use core::future::Future;
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

    /// Creates a new `Spawner` that will add tasks to the given executor state.
    #[must_use]
    pub(crate) fn with_executor_state(state: Weak<RefCell<ExecutorState<'a>>>) -> Self {
        Self { state }
    }

    /// Adds the given future to the executor's task queue so that it will be
    /// polled when the executor is run.
    ///
    /// If the executor has been dropped, this method will return the future
    /// back to the caller.
    ///
    /// This method is unsafe because TODO
    pub unsafe fn spawn_pinned(
        &self,
        future: Pin<Box<dyn Future<Output = ()>>>,
    ) -> Result<(), Pin<Box<dyn Future<Output = ()> + 'a>>> {
        // if let Some(state) = self.state.upgrade() {
        //     state.borrow_mut().spawn(future);
        // }
        todo!()
    }

    // TODO spawn method that takes a non-pinned future that may return a non-unit output
}
