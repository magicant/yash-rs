// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! Utilities for job control

use yash_env::Env;
use yash_env::job::{Job, Pid, ProcessResult};
use yash_env::semantics::ExitStatus;

/// Adds a job if the process is suspended.
///
/// This is a convenience function for handling the result of
/// [`Subshell::start_and_wait`](yash_env::subshell::Subshell::start_and_wait).
///
/// If the process result indicates that the process is stopped, this function
/// adds a job to the job list. The job is marked as job-controlled and its
/// state is derived from the process result. The job name is set to the result
/// of the `name` closure.
///
/// If the process is not stopped, this function does not add a job.
///
/// Returns the exit status of the process that should be assigned to
/// `env.exit_status`.
pub fn add_job_if_suspended<F>(
    env: &mut Env,
    pid: Pid,
    result: ProcessResult,
    name: F,
) -> crate::Result<ExitStatus>
where
    F: FnOnce() -> String,
{
    if result.is_stopped() {
        let mut job = Job::new(pid);
        job.job_controlled = true;
        job.state = result.into();
        job.name = name();
        env.jobs.add(job);
    }

    // TODO Break if stopped
    // TODO What if non-interactive?

    crate::Result::Continue(result.into())
}
