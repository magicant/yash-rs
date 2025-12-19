// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Wait built-in
//!
//! This module implements the [`wait` built-in], which waits for asynchronous
//! jobs to finish.
//!
//! [`wait` built-in]: https://magicant.github.io/yash-rs/builtins/wait.html
//!
//! # Implementation notes
//!
//! The built-in expects that an instance of
//! [`RunSignalTrapIfCaught`](yash_env::trap::RunSignalTrapIfCaught) is stored
//! in [`Env::any`] to handle trapped signals while waiting for jobs. If there
//! is no such instance, the built-in will ignore all signals.

use crate::common::report::{merge_reports, report_error, report_simple_failure};
use itertools::Itertools as _;
use yash_env::Env;
use yash_env::job::Pid;
use yash_env::option::State::Off;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::system::System;

/// Job specification (job ID or process ID)
///
/// Each operand of the `wait` built-in is parsed into a `JobSpec` value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobSpec {
    /// Process ID (non-negative decimal integer)
    ProcessId(Pid),

    /// Job ID (string of the form `%…`)
    JobId(Field),
}

/// Parsed command line arguments to the `wait` built-in
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    /// Operands that specify which jobs to wait for
    ///
    /// If empty, the built-in waits for all existing asynchronous jobs.
    pub jobs: Vec<JobSpec>,
}

pub mod core;
pub mod search;
pub mod status;
pub mod syntax;

impl Command {
    /// Waits for jobs specified by the indexes.
    ///
    /// If `indexes` is empty, waits for all jobs.
    async fn await_jobs<S, I>(env: &mut Env<S>, indexes: I) -> Result<ExitStatus, core::Error>
    where
        S: System + 'static,
        I: IntoIterator<Item = Option<usize>>,
    {
        // Currently, we ignore the job control option as required by POSIX.
        // TODO: Add some way to specify this option
        let job_control = Off; // env.options.get(Monitor);

        // Await jobs specified by the indexes
        let mut exit_status = None;
        for index in indexes {
            exit_status = Some(match index {
                None => ExitStatus::NOT_FOUND,
                Some(index) => {
                    status::wait_while_running(env, &mut status::job_status(index, job_control))
                        .await?
                }
            });
        }
        if let Some(exit_status) = exit_status {
            return Ok(exit_status);
        }

        // If there were no indexes, await all jobs
        status::wait_while_running(env, &mut status::any_job_is_running(job_control)).await
    }

    /// Executes the `wait` built-in.
    pub async fn execute<S: System + 'static>(self, env: &mut Env<S>) -> crate::Result {
        // Resolve job specifications to indexes
        let jobs = self.jobs.into_iter();
        let (indexes, errors): (Vec<_>, Vec<_>) = jobs
            .map(|spec| search::resolve(&env.jobs, spec))
            .partition_result();
        if let Some(report) = merge_reports(&errors) {
            return report_error(env, report).await;
        }

        // Await jobs specified by the indexes
        match Self::await_jobs(env, indexes).await {
            Ok(exit_status) => exit_status.into(),
            Err(core::Error::Trapped(signal, divert)) => {
                crate::Result::with_exit_status_and_divert(ExitStatus::from(signal), divert)
            }
            Err(error) => report_simple_failure(env, &error.to_string()).await,
        }
    }
}

/// Entry point for executing the `wait` built-in
pub async fn main<S: System + 'static>(env: &mut Env<S>, args: Vec<Field>) -> crate::Result {
    match syntax::parse(env, args) {
        Ok(command) => command.execute(env).await,
        Err(error) => report_error(env, &error).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::poll;
    use std::pin::pin;
    use std::task::Poll;
    use yash_env::job::{Job, ProcessResult};
    use yash_env::option::{Monitor, On};
    use yash_env::subshell::{JobControl, Subshell};
    use yash_env::system::r#virtual::SIGSTOP;
    use yash_env::trap::RunSignalTrapIfCaught;
    use yash_env_test_helper::{in_virtual_system, stub_tty};

    pub(super) fn stub_run_signal_trap_if_caught<S: 'static>(env: &mut Env<S>) {
        env.any.insert(Box::new(RunSignalTrapIfCaught::<S>(|_, _| {
            Box::pin(std::future::ready(None))
        })));
    }

    async fn suspend<S: System>(env: &mut Env<S>) {
        let target = env.system.getpid();
        env.system.kill(target, Some(SIGSTOP)).await.unwrap();
    }

    async fn start_self_suspending_job<S: System>(env: &mut Env<S>) {
        let subshell =
            Subshell::new(|env, _| Box::pin(suspend(env))).job_control(JobControl::Foreground);
        let (pid, subshell_result) = subshell.start_and_wait(env).await.unwrap();
        assert_eq!(subshell_result, ProcessResult::Stopped(SIGSTOP));
        let mut job = Job::new(pid);
        job.job_controlled = true;
        job.state = subshell_result.into();
        env.jobs.add(job);
    }

    #[test]
    fn suspended_job() {
        // Suspended jobs are not treated as finished, so the built-in waits indefinitely.
        in_virtual_system(|mut env, state| async move {
            stub_tty(&state);
            stub_run_signal_trap_if_caught(&mut env);
            env.options.set(Monitor, On);
            start_self_suspending_job(&mut env).await;

            let main = pin!(async move { main(&mut env, vec![]).await });
            let poll = poll!(main);
            assert_eq!(poll, Poll::Pending);
        })
    }
}
