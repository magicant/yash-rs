// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Methods for running tasks with concurrency

#[cfg(unix)]
use super::super::real::RealSystem;
use super::super::r#virtual::VirtualSystem;
use super::Concurrent;
use crate::job::ProcessState;
use futures_util::{pending, poll};
use std::pin::pin;

#[cfg(unix)]
impl Concurrent<RealSystem> {
    /// Runs the given task with concurrency support.
    ///
    /// This function implements the main loop of the shell process. It runs the
    /// given task while also calling [`select`](Self::select) to handle signals
    /// and other events. The task is expected to perform I/O operations using
    /// the methods of this `Concurrent` instance, so that it can yield when the
    /// operations would block. The function returns the output of the task when
    /// it completes.
    ///
    /// This method supports concurrency only inside the task. Other tasks
    /// created outside the task will not be run concurrently.
    /// This method blocks the current thread until the task completes, so it
    /// should only be called in the main function of the shell process.
    /// See the [`run_virtual`](Self::run_virtual) method for the
    /// `VirtualSystem` counterpart.
    pub fn run_real<F, T>(&self, task: F) -> T
    where
        F: Future<Output = T>,
    {
        use std::task::Poll::{Pending, Ready};
        use std::task::{Context, Waker};

        let runner = pin!(async move {
            let mut task = pin!(task);
            loop {
                if let Ready(result) = poll!(&mut task) {
                    return result;
                }
                self.select().await;
            }
        });
        match runner.poll(&mut Context::from_waker(Waker::noop())) {
            Ready(result) => result,
            Pending => unreachable!("`RealSystem::select` should never return `Pending`"),
        }
    }
}

impl Concurrent<VirtualSystem> {
    /// Runs the given task with concurrency support.
    ///
    /// This function implements the main loop of the shell process. It runs the
    /// given task while also calling [`select`](Self::select) to handle signals
    /// and other events. The task is expected to perform I/O operations using
    /// the methods of this `Concurrent` instance, so that it can yield when the
    /// operations would block. The function returns the output of the task when
    /// it completes.
    ///
    /// This is the `VirtualSystem` counterpart for the
    /// [`run_real`](Self::run_real) method. To allow `VirtualSystem` to run
    /// multiple tasks concurrently, this method is asynchronous and returns a
    /// future that completes when the task finishes or the process is
    /// terminated.
    pub async fn run_virtual<F>(&self, task: F)
    where
        F: Future<Output = ()>,
    {
        let mut task = pin!(task);
        while poll!(&mut task).is_pending() {
            let state = self.inner.current_process().state();
            match state {
                ProcessState::Running => {
                    // The process is running, but the task is not ready yet, so we need to wait
                    // for it to become ready. Proceed to the `select` call below.
                }
                ProcessState::Halted(result) => {
                    if result.is_stopped() {
                        // The process is stopped while the task is still working.
                        let terminated = self.inner.block_while_stopped().await;
                        if !terminated {
                            // The process has been resumed, so we can continue running the task.
                            continue;
                        }
                    }
                    // The process has been terminated, so we simply abort the task.
                    return;
                }
            }

            let mut select = pin!(self.select());
            while poll!(&mut select).is_pending() {
                let state = self.inner.current_process().state();
                match state {
                    ProcessState::Running => {
                        // The process is running, but the select call is not ready yet, so we need
                        // to wait for it to become ready. Here we propagate the pending state to
                        // the caller to yield to other processes.
                        pending!()
                    }
                    ProcessState::Halted(result) => {
                        if result.is_stopped() {
                            // The process is stopped while we are waiting for the select call.
                            let terminated = self.inner.block_while_stopped().await;
                            if !terminated {
                                // The process has been resumed, so we can continue waiting
                                // for the select call.
                                continue;
                            }
                        }
                        // The process has been terminated, so we simply abort the task.
                        return;
                    }
                }
            }
        }
    }
}
