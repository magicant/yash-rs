// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Implementation of the wait built-in core logic
//!
//! The [`wait_for_any_job_or_trap`] function waits for a job status change or
//! trap action. The [`Error`](enum@Error) type represents errors that may occur
//! in the function.

use thiserror::Error;
use yash_env::job::Pid;
use yash_env::system::Errno;
use yash_env::trap::Signal;
use yash_env::Env;
use yash_env::System as _;
use yash_semantics::trap::run_trap_if_caught;

/// Errors that may occur while waiting for a job
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum Error {
    /// There is no job to wait for.
    #[error("no job to wait for")]
    NothingToWait,
    /// The built-in was interrupted by a signal and the trap action was
    /// executed.
    #[error("trapped({0})")]
    Trapped(Signal, yash_env::semantics::Result),
    /// An unexpected error occurred in the underlying system.
    #[error("system error: {0}")]
    SystemError(#[from] Errno),
}

/// Waits for a job status change or trap.
///
/// This function waits for a next event, which is either an update of a job
/// status or a trap action. If the event is a job status change, this function
/// returns `Ok(())`. Otherwise, this function performs the trap action and
/// returns the signal and the result of the trap action.
///
/// Note that this function returns on a job status change of any kind. You need
/// to call this function repeatedly until the desired job status change occurs.
///
/// If there is no job to wait for, this function returns
/// `Err(Error::NothingToWait)` immediately.
pub async fn wait_for_any_job_or_trap(env: &mut Env) -> Result<(), Error> {
    // We need to set the signal handling before calling `wait` so we don't miss
    // any `SIGCHLD` that may arrive between `wait` and `wait_for_signals`.
    // See also Env::wait_for_subshell.
    env.traps.enable_sigchld_handler(&mut env.system)?;

    loop {
        // Poll for a job status change. Note that this `wait` call returns
        // immediately regardless of whether there is a new job status.
        match env.system.wait(Pid::from_raw(-1)) {
            Ok(None) => {
                // The current process has child processes, but none of them has
                // changed its status. Wait for a signal.
                let signals = env.wait_for_signals().await;
                for signal in signals.iter().cloned() {
                    if let Some(result) = run_trap_if_caught(env, signal).await {
                        return Err(Error::Trapped(signal, result));
                    }
                }
            }

            Ok(Some((pid, state))) => {
                // Some job has changed its state.
                env.jobs.update_status(state.to_wait_status(pid));
                return Ok(());
            }

            // The current process has no child processes.
            Err(Errno::ECHILD) => return Err(Error::NothingToWait),

            // Unexpected error
            Err(errno) => return Err(Error::SystemError(errno)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::in_virtual_system;
    use futures_util::poll;
    use futures_util::FutureExt as _;
    use std::future::{pending, ready};
    use std::ops::ControlFlow::Continue;
    use std::pin::pin;
    use std::task::Poll;
    use yash_env::job::Job;
    use yash_env::job::ProcessState;
    use yash_env::semantics::ExitStatus;
    use yash_env::subshell::Subshell;
    use yash_env::trap::Action;
    use yash_env::variable::Value;
    use yash_env::VirtualSystem;
    use yash_syntax::source::Location;

    #[test]
    fn running_job() {
        in_virtual_system(|mut env, _| async move {
            // Start a child process that never exits.
            let subshell = Subshell::new(|_, _| Box::pin(pending()));
            subshell.start(&mut env).await.unwrap();

            // The job is not finished, so the function keeps waiting.
            let future = pin!(wait_for_any_job_or_trap(&mut env));
            assert_eq!(poll!(future), Poll::Pending);
        });
    }

    #[test]
    fn finished_job() {
        in_virtual_system(|mut env, _| async move {
            // Start a child process that exits immediately.
            let subshell = Subshell::new(|_, _| Box::pin(ready(Continue(()))));
            let pid = subshell.start(&mut env).await.unwrap().0;
            let index = env.jobs.add(Job::new(pid));

            // The job is finished, so the function returns immediately.
            let result = wait_for_any_job_or_trap(&mut env).await;
            assert_eq!(result, Ok(()));
            // The job status is updated.
            assert_eq!(
                env.jobs.get(index).unwrap().state,
                ProcessState::Exited(ExitStatus::default()),
            );
        });
    }

    #[test]
    fn suspended_job() {
        in_virtual_system(|mut env, _| async move {
            // Start a child process that never exits.
            let subshell = Subshell::new(|_, _| Box::pin(pending()));
            let pid = subshell.start(&mut env).await.unwrap().0;
            let index = env.jobs.add(Job::new(pid));
            // Suspend the child process.
            env.system.kill(pid, Some(Signal::SIGSTOP)).await.unwrap();

            // The job is suspended, so the function returns immediately.
            let result = wait_for_any_job_or_trap(&mut env).await;
            assert_eq!(result, Ok(()));
            // The job status is updated.
            assert_eq!(
                env.jobs.get(index).unwrap().state,
                ProcessState::Stopped(Signal::SIGSTOP),
            );
        });
    }

    #[test]
    fn trap() {
        in_virtual_system(|mut env, state| async move {
            let mut system = VirtualSystem {
                state,
                process_id: env.main_pid,
            };

            // Start a child process that never exits.
            let subshell = Subshell::new(|_, _| Box::pin(pending()));
            subshell.start(&mut env).await.unwrap();

            // Set a trap for SIGTERM.
            env.traps
                .set_action(
                    &mut env.system,
                    Signal::SIGTERM,
                    Action::Command("foo=bar".into()),
                    Location::dummy("somewhere"),
                    false,
                )
                .unwrap();

            {
                // The job is not finished, so the function keeps waiting.
                let mut future = pin!(wait_for_any_job_or_trap(&mut env));
                assert_eq!(poll!(&mut future), Poll::Pending);

                // Trigger the trap.
                _ = system.current_process_mut().raise_signal(Signal::SIGTERM);

                // Now the function should return.
                let result = future.await;
                assert_eq!(result, Err(Error::Trapped(Signal::SIGTERM, Continue(()))));
            }

            // The trap action must have assigned the variable.
            assert_eq!(
                env.variables.get("foo").unwrap().value,
                Some(Value::scalar("bar")),
            );
        });
    }

    #[test]
    fn no_child_processes() {
        let mut env = Env::new_virtual();
        let result = wait_for_any_job_or_trap(&mut env).now_or_never().unwrap();
        assert_eq!(result, Err(Error::NothingToWait));
    }
}
