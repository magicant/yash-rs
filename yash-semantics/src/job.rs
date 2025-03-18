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

use std::ops::ControlFlow::{Break, Continue};
use yash_env::Env;
use yash_env::job::{Job, Pid, ProcessResult};
use yash_env::semantics::{Divert, ExitStatus};

/// Adds a job if the process is suspended.
///
/// This is a convenience function for handling the result of
/// [`Subshell::start_and_wait`](yash_env::subshell::Subshell::start_and_wait).
///
/// If the process result indicates that the process is stopped, this function
/// adds a job to the job list. The job is marked as job-controlled and its
/// state is derived from the process result. The job name is set to the result
/// of the `name` closure. If the current environment is interactive, this
/// function returns `Break(Divert::Interrupt(Some(exit_status)))` to indicate
/// that the shell should be interrupted.
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
    let exit_status = result.into();

    if result.is_stopped() {
        let mut job = Job::new(pid);
        job.job_controlled = true;
        job.state = result.into();
        job.name = name();
        env.jobs.add(job);

        if env.is_interactive() {
            return Break(Divert::Interrupt(Some(exit_status)));
        }
    }

    Continue(exit_status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZero;
    use yash_env::job::ProcessState;
    use yash_env::option::Option::Interactive;
    use yash_env::option::State::On;
    use yash_env::signal;

    #[test]
    fn do_not_add_job_if_exited() {
        let mut env = Env::new_virtual();
        let result = add_job_if_suspended(
            &mut env,
            Pid(123),
            ProcessResult::Exited(ExitStatus(42)),
            || "foo".to_string(),
        );
        assert_eq!(result, Continue(ExitStatus(42)));
        assert_eq!(env.jobs.len(), 0);
    }

    #[test]
    fn do_not_add_job_if_signaled() {
        let mut env = Env::new_virtual();
        let signal = signal::Number::from_raw_unchecked(NonZero::new(42).unwrap());
        let result = add_job_if_suspended(
            &mut env,
            Pid(123),
            ProcessResult::Signaled {
                signal,
                core_dump: false,
            },
            || "foo".to_string(),
        );
        assert_eq!(result, Continue(ExitStatus(42 + 0x180)));
        assert_eq!(env.jobs.len(), 0);
    }

    #[test]
    fn add_job_if_stopped() {
        let mut env = Env::new_virtual();
        let signal = signal::Number::from_raw_unchecked(NonZero::new(42).unwrap());
        let process_result = ProcessResult::Stopped(signal);
        let result = add_job_if_suspended(&mut env, Pid(123), process_result, || "foo".to_string());
        assert_eq!(result, Continue(ExitStatus(42 + 0x180)));
        assert_eq!(env.jobs.len(), 1);
        let job = env.jobs.get(0).unwrap();
        assert_eq!(job.pid, Pid(123));
        assert!(job.job_controlled);
        assert_eq!(job.state, ProcessState::Halted(process_result));
        assert_eq!(job.name, "foo");
    }

    #[test]
    fn break_if_stopped_and_interactive() {
        let mut env = Env::new_virtual();
        env.options.set(Interactive, On);
        let signal = signal::Number::from_raw_unchecked(NonZero::new(42).unwrap());
        let process_result = ProcessResult::Stopped(signal);
        let result = add_job_if_suspended(&mut env, Pid(123), process_result, || "foo".to_string());
        assert_eq!(
            result,
            Break(Divert::Interrupt(Some(ExitStatus(42 + 0x180))))
        );
        assert_eq!(env.jobs.len(), 1);
    }
}
