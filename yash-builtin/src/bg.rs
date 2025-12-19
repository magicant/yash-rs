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

//! Bg built-in
//!
//! This module implements the [`bg` built-in], which resumes suspended jobs in
//! the background.
//!
//! [`bg` built-in]: https://magicant.github.io/yash-rs/builtins/bg.html
//!
//! # Implementation notes
//!
//! This implementation sends the `SIGCONT` signal even to jobs that are already
//! running. The signal is not sent to jobs that have already terminated, to
//! prevent unrelated processes that happen to have the same process IDs as the
//! jobs from receiving the signal.
//!
//! The built-in sets the [expected state] of the resumed jobs to
//! [`ProcessState::Running`] so that the status changes are not reported again
//! on the next command prompt.
//!
//! [expected state]: yash_env::job::Job::expected_state

use crate::common::report::{merge_reports, report_error, report_failure, report_simple_failure};
use crate::common::syntax::Mode;
use crate::common::syntax::parse_arguments;
use std::fmt::Display;
use thiserror::Error;
use yash_env::Env;
use yash_env::System;
use yash_env::io::Fd;
#[cfg(doc)]
use yash_env::job::JobList;
use yash_env::job::ProcessState;
use yash_env::job::id::FindError;
use yash_env::job::id::ParseError;
use yash_env::job::id::parse;
use yash_env::option::Option::Monitor;
use yash_env::option::State::Off;
use yash_env::semantics::Field;
use yash_env::signal;
use yash_env::source::pretty::{Report, ReportType, Snippet};
use yash_env::system::Errno;

// Some definitions in this module are shared with the `fg` built-in.

/// Errors that may occur when resuming a job
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub(crate) enum ResumeError {
    #[error("target job is not controlled by the current shell environment")]
    Unowned,
    #[error("target job is not job-controlled")]
    Unmonitored,
    #[error("system error: {0}")]
    SystemError(#[from] Errno),
}

/// Errors that may occur when processing an operand
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub(crate) enum OperandErrorKind {
    /// The operand is not a job ID.
    #[error(transparent)]
    InvalidJobId(#[from] ParseError),
    /// The job ID does not specify a single job.
    #[error(transparent)]
    UnidentifiedJob(#[from] FindError),
    /// The job cannot be resumed.
    #[error(transparent)]
    CannotResume(#[from] ResumeError),
}

/// An operand and the error that occurred when processing it
#[derive(Clone, Debug, Error, Eq, PartialEq)]
struct OperandError(Field, OperandErrorKind);

impl Display for OperandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.0.value, self.1)
    }
}

impl OperandError {
    /// Converts the error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = "cannot resume job".into();
        let label = format!("{}: {}", self.0.value, self.1).into();
        report.snippets = Snippet::with_primary_span(&self.0.origin, label);
        report
    }
}

impl<'a> From<&'a OperandError> for Report<'a> {
    #[inline]
    fn from(error: &'a OperandError) -> Self {
        error.to_report()
    }
}

/// Resumes the job at the specified index.
///
/// This function panics if there is no job at the specified index.
async fn resume_job_by_index<S>(env: &mut Env<S>, index: usize) -> Result<(), ResumeError>
where
    S: System,
{
    let mut job = env.jobs.get_mut(index).unwrap();
    if !job.is_owned {
        return Err(ResumeError::Unowned);
    }
    if !job.job_controlled {
        return Err(ResumeError::Unmonitored);
    }

    let line = format!("[{}] {}\n", index + 1, job.name);
    env.system.write_all(Fd::STDOUT, line.as_bytes()).await?;
    drop(line);

    if job.state.is_alive() {
        let pgid = -job.pid;
        let sigcont = env.system.signal_number_from_name(signal::Name::Cont);
        let sigcont = sigcont.ok_or(Errno::EINVAL)?;
        env.system.kill(pgid, Some(sigcont)).await?;

        // We've just reported that the job is resumed, so there is no need to
        // report the same thing in the usual pre-prompt message.
        job.expect(ProcessState::Running);
    }

    let pid = job.pid;
    env.jobs.set_last_async_pid(pid);

    // The resumed job becomes the current job. This is only relevant when all
    // jobs are running since the current job is not changed if there is another
    // suspended job, but it is simpler to always update the current job.
    env.jobs.set_current_job(index).ok();

    Ok(())
}

/// Resumes the job specified by the operand.
async fn resume_job_by_id<S>(env: &mut Env<S>, job_id: &str) -> Result<(), OperandErrorKind>
where
    S: System,
{
    let job_id = parse(job_id)?;
    let index = job_id.find(&env.jobs)?;
    resume_job_by_index(env, index).await?;
    Ok(())
}

/// Entry point of the `bg` built-in
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> crate::Result
where
    S: System,
{
    let (options, operands) = match parse_arguments(&[], Mode::with_env(env), args) {
        Ok(result) => result,
        Err(error) => return report_error(env, &error).await,
    };
    debug_assert_eq!(options, []);

    if env.options.get(Monitor) == Off {
        return report_simple_failure(env, "job control is disabled").await;
    }

    if operands.is_empty() {
        if let Some(index) = env.jobs.current_job() {
            match resume_job_by_index(env, index).await {
                Ok(()) => crate::Result::default(),
                Err(error) => report_simple_failure(env, &error.to_string()).await,
            }
        } else {
            report_simple_failure(env, "there is no job").await
        }
    } else {
        let mut errors = Vec::new();
        for operand in operands {
            match resume_job_by_id(env, &operand.value).await {
                Ok(()) => {}
                Err(error) => errors.push(OperandError(operand, error)),
            }
        }
        match merge_reports(&errors) {
            None => crate::Result::default(),
            Some(report) => report_failure(env, report).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use yash_env::VirtualSystem;
    use yash_env::job::Job;
    use yash_env::job::Pid;
    use yash_env::job::ProcessState;
    use yash_env::option::State::On;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::r#virtual::Process;
    use yash_env::system::r#virtual::{SIGSTOP, SIGTSTP, SIGTTIN};
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;

    #[test]
    fn resume_job_by_index_sends_sigcont() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(system.clone());
        let pgid = Pid(123);
        let child_id = Pid(124);
        let orphan_id = Pid(456);
        let mut job = Job::new(pgid);
        let mut orphan = Job::new(orphan_id);
        job.job_controlled = true;
        orphan.job_controlled = true;
        let index = env.jobs.add(job);
        let _ = env.jobs.add(orphan);
        let mut leader = Process::with_parent_and_group(system.process_id, pgid);
        let mut child = Process::fork_from(pgid, &leader);
        let mut orphan = Process::with_parent_and_group(system.process_id, orphan_id);
        _ = leader.set_state(ProcessState::stopped(SIGTTIN));
        _ = child.set_state(ProcessState::stopped(SIGTSTP));
        _ = orphan.set_state(ProcessState::stopped(SIGSTOP));
        {
            let mut state = system.state.borrow_mut();
            state.processes.insert(pgid, leader);
            state.processes.insert(child_id, child);
            state.processes.insert(orphan_id, orphan);
        }

        resume_job_by_index(&mut env, index)
            .now_or_never()
            .unwrap()
            .unwrap();

        let state = system.state.borrow();
        // The process group leader should be resumed.
        assert_eq!(state.processes[&pgid].state(), ProcessState::Running);
        // The child should also be resumed as it belongs to the same process group.
        assert_eq!(state.processes[&child_id].state(), ProcessState::Running);
        // Unrelated processes should not be resumed.
        assert_eq!(
            state.processes[&orphan_id].state(),
            ProcessState::stopped(SIGSTOP),
        );
    }

    #[test]
    fn resume_job_by_index_prints_job_name() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(system.clone());
        let mut job = Job::new(Pid(123));
        job.job_controlled = true;
        job.name = "echo my job".into();
        let index = env.jobs.add(job);

        _ = resume_job_by_index(&mut env, index).now_or_never().unwrap();

        assert_stdout(&system.state, |stdout| {
            assert_eq!(stdout, "[1] echo my job\n");
        });
        assert_stderr(&system.state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn resume_job_by_index_sets_expected_state() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(system.clone());
        let pid = Pid(123);
        let mut job = Job::new(pid);
        job.job_controlled = true;
        let index = env.jobs.add(job);
        let mut process = Process::with_parent_and_group(system.process_id, pid);
        _ = process.set_state(ProcessState::stopped(SIGSTOP));
        {
            let mut state = system.state.borrow_mut();
            state.processes.insert(pid, process);
        }

        _ = resume_job_by_index(&mut env, index).now_or_never().unwrap();

        assert_eq!(env.jobs[index].expected_state, Some(ProcessState::Running));
    }

    #[test]
    fn resume_job_by_index_makes_target_current_job() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(system.clone());
        let pgid = Pid(123);
        let orphan_id = Pid(456);
        let mut job = Job::new(pgid);
        let mut orphan = Job::new(orphan_id);
        job.job_controlled = true;
        orphan.job_controlled = true;
        let index = env.jobs.add(job);
        let orphan_index = env.jobs.add(orphan);
        env.jobs.set_current_job(orphan_index).unwrap();
        let mut leader = Process::with_parent_and_group(system.process_id, pgid);
        let mut orphan = Process::with_parent_and_group(system.process_id, orphan_id);
        _ = leader.set_state(ProcessState::stopped(SIGTTIN));
        _ = orphan.set_state(ProcessState::Running);
        {
            let mut state = system.state.borrow_mut();
            state.processes.insert(pgid, leader);
            state.processes.insert(orphan_id, orphan);
        }

        resume_job_by_index(&mut env, index)
            .now_or_never()
            .unwrap()
            .unwrap();

        assert_eq!(env.jobs.current_job(), Some(index));
    }

    #[test]
    fn resume_job_by_index_sends_no_sigcont_to_dead_process() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(system.clone());
        let pid = Pid(123);
        let mut job = Job::new(pid);
        job.job_controlled = true;
        job.state = ProcessState::exited(ExitStatus::SUCCESS);
        let index = env.jobs.add(job);
        // This process (irrelevant to the job) happens to have the same PID as the job.
        let mut process = Process::with_parent_and_group(system.process_id, pid);
        _ = process.set_state(ProcessState::stopped(SIGSTOP));
        {
            let mut state = system.state.borrow_mut();
            state.processes.insert(pid, process);
        }

        resume_job_by_index(&mut env, index)
            .now_or_never()
            .unwrap()
            .unwrap();

        let state = system.state.borrow();
        // The process should not be resumed.
        assert_eq!(
            state.processes[&pid].state(),
            ProcessState::stopped(SIGSTOP),
        );
    }

    #[test]
    fn resume_job_by_index_sets_last_async_pid() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(system.clone());
        let mut job = Job::new(Pid(123));
        job.job_controlled = true;
        job.state = ProcessState::exited(ExitStatus::SUCCESS);
        let index = env.jobs.add(job);

        _ = resume_job_by_index(&mut env, index).now_or_never().unwrap();

        assert_eq!(env.jobs.last_async_pid(), Pid(123));
    }

    #[test]
    fn resume_job_by_index_rejects_unowned_job() {
        let mut env = Env::new_virtual();
        let mut job = Job::new(Pid(123));
        job.job_controlled = true;
        job.is_owned = false;
        let index = env.jobs.add(job);

        let result = resume_job_by_index(&mut env, index).now_or_never().unwrap();
        assert_eq!(result, Err(ResumeError::Unowned));
    }

    #[test]
    fn resume_job_by_index_rejects_unmonitored_job() {
        let mut env = Env::new_virtual();
        let index = env.jobs.add(Job::new(Pid(123)));

        let result = resume_job_by_index(&mut env, index).now_or_never().unwrap();
        assert_eq!(result, Err(ResumeError::Unmonitored));
    }

    #[test]
    fn main_without_operands_resumes_current_job() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(system.clone());
        env.options.set(Monitor, On);
        let pgid = Pid(100);
        let orphan_id = Pid(200);
        let mut job = Job::new(pgid);
        let mut orphan = Job::new(orphan_id);
        job.job_controlled = true;
        orphan.job_controlled = true;
        let _ = env.jobs.add(orphan);
        let index = env.jobs.add(job);
        env.jobs.set_current_job(index).unwrap();
        let mut leader = Process::with_parent_and_group(system.process_id, pgid);
        let mut orphan = Process::with_parent_and_group(system.process_id, orphan_id);
        _ = leader.set_state(ProcessState::stopped(SIGSTOP));
        _ = orphan.set_state(ProcessState::stopped(SIGTTIN));
        {
            let mut state = system.state.borrow_mut();
            state.processes.insert(pgid, leader);
            state.processes.insert(orphan_id, orphan);
        }

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, crate::Result::default());

        let state = system.state.borrow();
        // The current job's process group leader should be resumed.
        assert_eq!(state.processes[&pgid].state(), ProcessState::Running);
        // Unrelated processes should not be resumed.
        assert_eq!(
            state.processes[&orphan_id].state(),
            ProcessState::stopped(SIGTTIN),
        );
        // No error message should be printed on success.
        assert_stderr(&system.state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn main_without_operands_fails_if_there_is_no_current_job() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(system.clone());
        env.options.set(Monitor, On);

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, crate::Result::from(ExitStatus::FAILURE));

        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("there is no job"), "{stderr:?}");
        });
    }

    #[test]
    fn main_with_operands_resumes_specified_jobs() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(system.clone());
        env.options.set(Monitor, On);
        let pgid1 = Pid(100);
        let pgid2 = Pid(200);
        let pgid3 = Pid(300);
        let mut job1 = Job::new(pgid1);
        let mut job2 = Job::new(pgid2);
        let mut job3 = Job::new(pgid3);
        job1.job_controlled = true;
        job2.job_controlled = true;
        job3.job_controlled = true;
        let _ = env.jobs.add(job1);
        let _ = env.jobs.add(job2);
        let _ = env.jobs.add(job3);
        let mut process1 = Process::with_parent_and_group(system.process_id, pgid1);
        let mut process2 = Process::with_parent_and_group(system.process_id, pgid2);
        let mut process3 = Process::with_parent_and_group(system.process_id, pgid3);
        _ = process1.set_state(ProcessState::stopped(SIGSTOP));
        _ = process2.set_state(ProcessState::stopped(SIGSTOP));
        _ = process3.set_state(ProcessState::stopped(SIGSTOP));
        {
            let mut state = system.state.borrow_mut();
            state.processes.insert(pgid1, process1);
            state.processes.insert(pgid2, process2);
            state.processes.insert(pgid3, process3);
        }

        let result = main(&mut env, Field::dummies(["%1", "%3"]))
            .now_or_never()
            .unwrap();
        assert_eq!(result, crate::Result::default());

        let state = system.state.borrow();
        // The specified jobs should be resumed.
        assert_eq!(state.processes[&pgid1].state(), ProcessState::Running);
        assert_eq!(state.processes[&pgid3].state(), ProcessState::Running);
        // Unrelated processes should not be resumed.
        assert_eq!(
            state.processes[&pgid2].state(),
            ProcessState::stopped(SIGSTOP),
        );
        // No error message should be printed on success.
        assert_stderr(&system.state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn main_with_operands_tries_to_resume_all_jobs() {
        // In this test case, only the second operand is valid. The main
        // function fails on the first operand, but it should try to handle the
        // remaining operands.
        let system = VirtualSystem::new();
        let mut env = Env::with_system(system.clone());
        env.options.set(Monitor, On);
        let pgid = Pid(100);
        let mut job = Job::new(pgid);
        job.job_controlled = true;
        let index = env.jobs.add(job);
        env.jobs.set_current_job(index).unwrap();
        let mut leader = Process::with_parent_and_group(system.process_id, pgid);
        _ = leader.set_state(ProcessState::stopped(SIGSTOP));
        {
            let mut state = system.state.borrow_mut();
            state.processes.insert(pgid, leader);
        }

        let result = main(&mut env, Field::dummies(["%2", "%1", "%3"]))
            .now_or_never()
            .unwrap();
        assert_eq!(result, crate::Result::from(ExitStatus::FAILURE));

        let state = system.state.borrow();
        // The job should be resumed.
        assert_eq!(state.processes[&pgid].state(), ProcessState::Running);
        // Some error messages should be printed for the invalid operands.
        assert_stderr(&system.state, |stderr| assert_ne!(stderr, ""));
    }
}
