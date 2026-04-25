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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    mod real_system {
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

    mod virtual_system {
        use super::*;
        use crate::semantics::ExitStatus;
        use crate::system::r#virtual::{SIGCONT, SIGKILL, SIGSTOP};
        use crate::system::{Exit as _, SendSignal as _};
        use crate::test_helper::WakeFlag;
        use futures_util::FutureExt as _;
        use std::cell::Cell;
        use std::rc::Rc;
        use std::sync::Arc;
        use std::task::Poll::{Pending, Ready};
        use std::task::{Context, Waker};
        use std::time::{Duration, Instant};

        struct DropFlag(Rc<Cell<bool>>);

        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        fn virtual_system_with_current_time() -> (Concurrent<VirtualSystem>, Instant) {
            let inner = VirtualSystem::new();
            let now = Instant::now();
            inner.state.borrow_mut().now = Some(now);
            (Concurrent::new(inner), now)
        }

        #[test]
        fn run_virtual_returns_immediately_when_task_is_ready_on_first_poll() {
            let system = Concurrent::new(VirtualSystem::new());
            let completed = Cell::new(false);

            let result = system
                .run_virtual(async { completed.set(true) })
                .now_or_never();

            assert_eq!(result, Some(()));
            assert!(completed.get());
        }

        #[test]
        fn run_virtual_completes_normally_when_task_alternates_between_pending_and_ready() {
            let (system, now) = virtual_system_with_current_time();
            let progress = Rc::new(Cell::new(0));
            let progress_2 = Rc::clone(&progress);
            let mut future = pin!(system.run_virtual(async {
                progress_2.set(1);
                system.sleep(Duration::from_secs(1)).await;
                progress_2.set(2);
                system.sleep(Duration::from_secs(1)).await;
                progress_2.set(3);
            }));

            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(future.as_mut().poll(&mut context), Pending);
            assert_eq!(progress.get(), 1);

            system
                .inner
                .state
                .borrow_mut()
                .advance_time(now + Duration::from_secs(1));
            assert_eq!(future.as_mut().poll(&mut context), Pending);
            assert_eq!(progress.get(), 2);

            system
                .inner
                .state
                .borrow_mut()
                .advance_time(now + Duration::from_secs(2));
            assert_eq!(future.as_mut().poll(&mut context), Ready(()));
            assert_eq!(progress.get(), 3);
        }

        #[test]
        fn run_virtual_waits_on_select_while_process_is_running_and_task_is_pending() {
            let (system, now) = virtual_system_with_current_time();
            let completed = Rc::new(Cell::new(false));
            let completed_2 = Rc::clone(&completed);
            let mut future = pin!(system.run_virtual(async {
                system.sleep(Duration::from_secs(1)).await;
                completed_2.set(true);
            }));

            let wake_flag = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&wake_flag));
            let mut context = Context::from_waker(&waker);
            assert_eq!(future.as_mut().poll(&mut context), Pending);
            assert!(!completed.get());
            assert!(!wake_flag.is_woken());

            system
                .inner
                .state
                .borrow_mut()
                .advance_time(now + Duration::from_secs(1));
            assert!(wake_flag.is_woken());

            let wake_flag = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&wake_flag));
            let mut context = Context::from_waker(&waker);
            assert_eq!(future.as_mut().poll(&mut context), Ready(()));
            assert!(completed.get());
            assert!(!wake_flag.is_woken());
        }

        #[test]
        fn run_virtual_yields_pending_to_caller_while_waiting_on_pending_select_in_running_state() {
            let (system, _now) = virtual_system_with_current_time();
            let completed = Rc::new(Cell::new(false));
            let completed_2 = Rc::clone(&completed);
            let mut future = pin!(system.run_virtual(async {
                system.sleep(Duration::from_secs(1)).await;
                completed_2.set(true);
            }));

            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(future.as_mut().poll(&mut context), Pending);
            assert_eq!(future.as_mut().poll(&mut context), Pending);
            assert!(!completed.get());
        }

        #[test]
        fn run_virtual_aborts_task_when_process_is_already_terminated_before_entering_select() {
            let system = Concurrent::new(VirtualSystem::new());
            let dropped = Rc::new(Cell::new(false));
            let dropped_2 = Rc::clone(&dropped);
            let mut future = pin!(system.run_virtual(async {
                let _drop_flag = DropFlag(dropped_2);
                system.exit(ExitStatus(42)).await;
            }));

            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(future.as_mut().poll(&mut context), Ready(()));
            assert!(dropped.get());
        }

        #[test]
        fn run_virtual_blocks_while_stopped_before_select_and_resumes_task_when_process_is_continued()
         {
            let system = Concurrent::new(VirtualSystem::new());
            let completed = Rc::new(Cell::new(false));
            let mut future = pin!(system.run_virtual(async {
                system.raise(SIGSTOP).await.unwrap();
                completed.set(true);
            }));

            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(future.as_mut().poll(&mut context), Pending);
            assert_eq!(
                system.inner.current_process().state(),
                ProcessState::stopped(SIGSTOP),
            );
            assert!(!completed.get());

            _ = system.inner.current_process_mut().raise_signal(SIGCONT);
            assert_eq!(future.as_mut().poll(&mut context), Ready(()));
            assert!(completed.get());
        }

        #[test]
        fn run_virtual_blocks_while_stopped_before_select_and_aborts_when_process_terminates() {
            let system = Concurrent::new(VirtualSystem::new());
            let dropped = Rc::new(Cell::new(false));
            let dropped_2 = Rc::clone(&dropped);
            let mut future = pin!(system.run_virtual(async {
                let _drop_flag = DropFlag(dropped_2);
                system.raise(SIGSTOP).await.unwrap();
                unreachable!("task should be aborted while stopped");
            }));

            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(future.as_mut().poll(&mut context), Pending);
            assert!(!dropped.get());

            _ = system.inner.current_process_mut().raise_signal(SIGKILL);
            assert_eq!(future.as_mut().poll(&mut context), Ready(()));
            assert!(dropped.get());
        }

        #[test]
        fn run_virtual_blocks_while_stopped_during_pending_select_and_continues_waiting_after_resume()
         {
            let (system, now) = virtual_system_with_current_time();
            let completed = Rc::new(Cell::new(false));
            let completed_2 = Rc::clone(&completed);
            let mut future = pin!(system.run_virtual(async {
                system.sleep(Duration::from_secs(1)).await;
                completed_2.set(true);
            }));

            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(future.as_mut().poll(&mut context), Pending);

            _ = system
                .inner
                .current_process_mut()
                .set_state(ProcessState::stopped(SIGSTOP));
            assert_eq!(future.as_mut().poll(&mut context), Pending);
            assert!(!completed.get());

            system
                .inner
                .state
                .borrow_mut()
                .advance_time(now + Duration::from_secs(1));
            assert_eq!(future.as_mut().poll(&mut context), Pending);
            assert!(!completed.get());

            _ = system
                .inner
                .current_process_mut()
                .set_state(ProcessState::Running);
            assert_eq!(future.as_mut().poll(&mut context), Ready(()));
            assert!(completed.get());
        }

        #[test]
        fn run_virtual_blocks_while_stopped_during_pending_select_and_aborts_when_terminated() {
            let (system, _now) = virtual_system_with_current_time();
            let dropped = Rc::new(Cell::new(false));
            let mut future = pin!(system.run_virtual(async {
                let _drop_flag = DropFlag(Rc::clone(&dropped));
                system.sleep(Duration::from_secs(1)).await;
                unreachable!("task should be aborted while sleeping");
            }));

            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(future.as_mut().poll(&mut context), Pending);

            _ = system
                .inner
                .current_process_mut()
                .set_state(ProcessState::stopped(SIGSTOP));
            assert_eq!(future.as_mut().poll(&mut context), Pending);
            assert!(!dropped.get());

            _ = system.inner.current_process_mut().raise_signal(SIGKILL);
            assert_eq!(future.as_mut().poll(&mut context), Ready(()));
            assert!(dropped.get());
        }

        #[test]
        fn run_virtual_aborts_immediately_when_process_becomes_terminated_while_waiting_on_pending_select()
         {
            let (system, _now) = virtual_system_with_current_time();
            let dropped = Rc::new(Cell::new(false));
            let mut future = pin!(system.run_virtual(async {
                let _drop_flag = DropFlag(Rc::clone(&dropped));
                system.sleep(Duration::from_secs(1)).await;
                unreachable!("task should be aborted while sleeping");
            }));

            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(future.as_mut().poll(&mut context), Pending);
            assert!(!dropped.get());

            _ = system.inner.current_process_mut().raise_signal(SIGKILL);
            assert_eq!(future.as_mut().poll(&mut context), Ready(()));
            assert!(dropped.get());
        }
    }
}
