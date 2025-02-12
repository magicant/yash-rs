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
//! The **`wait`** built-in waits for asynchronous jobs to finish.
//!
//! # Synopsis
//!
//! ```sh
//! wait [job_id_or_process_id…]
//! ```
//!
//! # Description
//!
//! If you specify one or more operands, the built-in waits for the specified
//! jobs to finish. Otherwise, the built-in waits for all existing asynchronous
//! jobs. If the jobs are already finished, the built-in returns without
//! waiting.
//!
//! If you try to wait for a suspended job, the built-in will wait indefinitely
//! until the job is resumed and finished. Currently, there is no way to
//! cancel the wait.
//! (TODO: Add a way to cancel the wait)
//! (TODO: Add a way to treat a suspended job as if it were finished)
//!
//! # Options
//!
//! None
//!
//! # Operands
//!
//! An operand can be a job ID or decimal process ID, specifying which job to
//! wait for. A job ID must start with `%` and has the format described in the
//! [`yash_env::job::id`] module documentation. A process ID is a non-negative
//! decimal integer.
//!
//! If there is no job matching the operand, the built-in assumes that the
//! job has already finished with exit status 127.
//!
//! # Errors
//!
//! The following error conditions causes the built-in to return a non-zero exit
//! status without waiting for any job:
//!
//! - An operand is not a job ID or decimal process ID.
//! - A job ID matches more than one job.
//! - The shell receives a signal that has a [trap](yash_env::trap) action set.
//!
//! The trap action for the signal is executed before the built-in returns.
//!
//! # Exit status
//!
//! If you specify one or more operands, the built-in returns the exit status of
//! the job specified by the last operand. If there is no operand, the exit
//! status is 0 regardless of the awaited jobs.
//!
//! If the built-in was interrupted by a signal, the exit status indicates the
//! signal.
//!
//! The exit status is between 1 and 126 (inclusive) for any other error.
//!
//! # Portability
//!
//! The wait built-in is contained in the POSIX standard.
//!
//! The exact value of an exit status resulting from a signal is
//! implementation-dependent.
//!
//! Many existing shells behave differently on various errors. POSIX requires
//! that an unknown process ID be treated as a process that has already exited
//! with exit status 127, but the behavior for other errors should not be
//! considered portable.
//!
//! # Implementation notes
//!
//! The built-in treats disowned jobs as if they were finished with an exit
//! status of 127.

use crate::common::report_error;
use crate::common::report_simple_failure;
use crate::common::to_single_message;
use itertools::Itertools as _;
use yash_env::job::Pid;
use yash_env::option::State::Off;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::Env;

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
    async fn await_jobs<I>(env: &mut Env, indexes: I) -> Result<ExitStatus, core::Error>
    where
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
    pub async fn execute(self, env: &mut Env) -> crate::Result {
        // Resolve job specifications to indexes
        let jobs = self.jobs.into_iter();
        let (indexes, errors): (Vec<_>, Vec<_>) = jobs
            .map(|spec| search::resolve(&env.jobs, spec))
            .partition_result();
        if let Some(message) = to_single_message(&errors) {
            return report_error(env, message).await;
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
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    match syntax::parse(env, args) {
        Ok(command) => command.execute(env).await,
        Err(error) => report_error(env, &error).await,
    }
}
