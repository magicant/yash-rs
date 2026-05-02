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

//! Methods for running tasks with concurrency (`RealSystem`-specific)

#![cfg(unix)]

use super::super::real::RealSystem;
use super::Concurrent;
use futures_util::poll;
use std::pin::pin;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::time::Duration;

    #[test]
    fn run_real_returns_task_output_immediately_if_ready_on_first_poll() {
        let system = Concurrent::new(unsafe { RealSystem::new() });
        let result = system.run_real(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn run_real_keeps_polling_task_until_completion_when_task_yields_multiple_times() {
        let system = Concurrent::new(unsafe { RealSystem::new() });
        let progress = Cell::new(0);

        let result = system.run_real(async {
            progress.set(1);
            system.sleep(Duration::from_millis(1)).await;
            progress.set(2);
            system.sleep(Duration::from_millis(1)).await;
            progress.set(3);
            42
        });

        assert_eq!(result, 42);
        assert_eq!(progress.get(), 3);
    }

    #[test]
    fn run_real_calls_select_between_task_polls_while_task_is_pending() {
        let system = Concurrent::new(unsafe { RealSystem::new() });
        let progress = Cell::new(0);

        let result = system.run_real(async {
            progress.set(1);
            system.sleep(Duration::from_millis(1)).await;
            progress.set(2);
            7
        });

        assert_eq!(result, 7);
        assert_eq!(progress.get(), 2);
    }

    #[test]
    #[should_panic = "boom"]
    fn run_real_propagates_task_panic_to_caller() {
        let system = Concurrent::new(unsafe { RealSystem::new() });
        system.run_real(async { panic!("boom") })
    }
}
