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
//! The **`bg`** built-in resumes a suspended job in the background.
//!
//! # Synopsis
//!
//! ```sh
//! bg [job_id…]
//! ```
//!
//! # Description
//!
//! The built-in resumes the specified jobs by sending the `SIGCONT` signal to
//! them.
//!
//! # Options
//!
//! None.
//!
//! # Operands
//!
//! Operands specify which jobs to resume. See the module documentation of
//! [`yash_env::job::id`] for the format of job IDs. If omitted, the built-in
//! resumes the [current job](JobSet::current_job).
//!
//! (TODO: allow omitting the leading `%`)
//!
//! # Standard output
//!
//! The built-in writes the job number and name of each resumed job to the
//! standard output.
//!
//! # Errors
//!
//! It is an error if the specified job is not found, not job-controlled, or
//! not [owned] by the current shell environment.
//!
//! # Exit status
//!
//! Zero unless an error occurs.
//!
//! # Portability
//!
//! Many implementations allow omitting the leading `%` from job IDs, though it
//! is not required by POSIX.
//!
//! Some implementations (including the previous version of yash, but not this
//! version) regard it is an error to resume a job that has already terminated.
//!
//! # Implementation notes
//!
//! This implementation sends the `SIGCONT` signal even to jobs that are already
//! running. The signal is not sent to jobs that have already terminated, to
//! prevent unrelated processes that happen to have the same process IDs as the
//! jobs from receiving the signal.
//!
//! The built-in sets the [expected status] of the resumed jobs to
//! [`WaitStatus::Continued`] so that the status changes are not reported again
//! on the next command prompt.
//!
//! [owned]: yash_env::job::Job::is_owned
//! [expected status]: yash_env::job::Job::expected_status

use crate::common::report_error;
use crate::common::report_failure;
use crate::common::report_simple_failure;
use crate::common::syntax::parse_arguments;
use crate::common::syntax::Mode;
use crate::common::to_single_message;
use std::borrow::Cow;
use std::fmt::Display;
use thiserror::Error;
use yash_env::io::Fd;
use yash_env::job::fmt::Marker;
use yash_env::job::fmt::Report;
use yash_env::job::id::parse;
use yash_env::job::id::FindError;
use yash_env::job::id::ParseError;
#[cfg(doc)]
use yash_env::job::JobSet;
use yash_env::job::Pid;
use yash_env::job::WaitStatus;
use yash_env::semantics::Field;
use yash_env::system::Errno;
use yash_env::trap::Signal;
use yash_env::Env;
use yash_env::System;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;

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

impl MessageBase for OperandError {
    fn message_title(&self) -> Cow<str> {
        "cannot resume job".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        let label = format!("{}: {}", self.0.value, self.1).into();
        Annotation::new(AnnotationType::Error, label, &self.0.origin)
    }
}

/// Resumes the job at the specified index.
///
/// This function panics if there is no job at the specified index.
async fn resume_job_by_index(env: &mut Env, index: usize) -> Result<(), ResumeError> {
    let mut job = env.jobs.get_mut(index).unwrap();
    if !job.is_owned {
        return Err(ResumeError::Unowned);
    }
    if !job.job_controlled {
        return Err(ResumeError::Unmonitored);
    }

    let report = Report {
        index,
        marker: Marker::None,
        job: &job,
    };
    let line = format!("[{}] {}\n", report.number(), job.name);
    env.system.write_all(Fd::STDOUT, line.as_bytes()).await?;
    drop(line);

    if job.state.is_alive() {
        let pgid = Pid::from_raw(-job.pid.as_raw());
        env.system.kill(pgid, Signal::SIGCONT.into()).await?;

        // We've just reported that the job is resumed, so there is no need to
        // report the same thing in the usual pre-prompt message.
        job.expect(WaitStatus::Continued(job.pid));
    }

    // The resumed job becomes the current job. This is only relevant when all
    // jobs are running since the current job is not changed if there is another
    // suspended job, but it is simpler to always update the current job.
    env.jobs.set_current_job(index).ok();

    Ok(())
}

/// Resumes the job specified by the operand.
async fn resume_job_by_id(env: &mut Env, job_id: &str) -> Result<(), OperandErrorKind> {
    let job_id = parse(job_id)?;
    let index = job_id.find(&env.jobs)?;
    resume_job_by_index(env, index).await?;
    Ok(())
}

/// Entry point of the `bg` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    let (options, operands) = match parse_arguments(&[], Mode::with_env(env), args) {
        Ok(result) => result,
        Err(error) => return report_error(env, &error).await,
    };
    debug_assert_eq!(options, []);

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
        match to_single_message(&{ errors }) {
            None => crate::Result::default(),
            Some(message) => report_failure(env, message).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use futures_util::FutureExt as _;
    use yash_env::job::Job;
    use yash_env::job::ProcessState;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::r#virtual::Process;
    use yash_env::VirtualSystem;

    #[test]
    fn resume_job_by_index_sends_sigcont() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));
        let pgid = Pid::from_raw(123);
        let child_id = Pid::from_raw(124);
        let orphan_id = Pid::from_raw(456);
        let mut job = Job::new(pgid);
        let mut orphan = Job::new(orphan_id);
        job.job_controlled = true;
        orphan.job_controlled = true;
        let index = env.jobs.add(job);
        let _ = env.jobs.add(orphan);
        let mut leader = Process::with_parent_and_group(system.process_id, pgid);
        let mut child = Process::fork_from(pgid, &leader);
        let mut orphan = Process::with_parent_and_group(system.process_id, orphan_id);
        _ = leader.set_state(ProcessState::Stopped(Signal::SIGTTIN));
        _ = child.set_state(ProcessState::Stopped(Signal::SIGTSTP));
        _ = orphan.set_state(ProcessState::Stopped(Signal::SIGSTOP));
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
            ProcessState::Stopped(Signal::SIGSTOP),
        );
    }

    #[test]
    fn resume_job_by_index_prints_job_name() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));
        let mut job = Job::new(Pid::from_raw(123));
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
    fn resume_job_by_index_sets_expected_status() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));
        let pid = Pid::from_raw(123);
        let mut job = Job::new(pid);
        job.job_controlled = true;
        let index = env.jobs.add(job);
        let mut process = Process::with_parent_and_group(system.process_id, pid);
        _ = process.set_state(ProcessState::Stopped(Signal::SIGSTOP));
        {
            let mut state = system.state.borrow_mut();
            state.processes.insert(pid, process);
        }

        _ = resume_job_by_index(&mut env, index).now_or_never().unwrap();

        let job = env.jobs.get(index).unwrap();
        assert_eq!(job.expected_status, Some(WaitStatus::Continued(pid)));
    }

    #[test]
    fn resume_job_by_index_makes_target_current_job() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));
        let pgid = Pid::from_raw(123);
        let orphan_id = Pid::from_raw(456);
        let mut job = Job::new(pgid);
        let mut orphan = Job::new(orphan_id);
        job.job_controlled = true;
        orphan.job_controlled = true;
        let index = env.jobs.add(job);
        let orphan_index = env.jobs.add(orphan);
        env.jobs.set_current_job(orphan_index).unwrap();
        let mut leader = Process::with_parent_and_group(system.process_id, pgid);
        let mut orphan = Process::with_parent_and_group(system.process_id, orphan_id);
        _ = leader.set_state(ProcessState::Stopped(Signal::SIGTTIN));
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
        let mut env = Env::with_system(Box::new(system.clone()));
        let pid = Pid::from_raw(123);
        let mut job = Job::new(pid);
        job.job_controlled = true;
        job.state = ProcessState::Exited(ExitStatus::SUCCESS);
        let index = env.jobs.add(job);
        // This process (irrelevant to the job) happens to have the same PID as the job.
        let mut process = Process::with_parent_and_group(system.process_id, pid);
        _ = process.set_state(ProcessState::Stopped(Signal::SIGSTOP));
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
            ProcessState::Stopped(Signal::SIGSTOP),
        );
    }

    #[test]
    fn resume_job_by_index_rejects_unowned_job() {
        let mut env = Env::new_virtual();
        let mut job = Job::new(Pid::from_raw(123));
        job.job_controlled = true;
        job.is_owned = false;
        let index = env.jobs.add(job);

        let result = resume_job_by_index(&mut env, index).now_or_never().unwrap();
        assert_eq!(result, Err(ResumeError::Unowned));
    }

    #[test]
    fn resume_job_by_index_rejects_unmonitored_job() {
        let mut env = Env::new_virtual();
        let index = env.jobs.add(Job::new(Pid::from_raw(123)));

        let result = resume_job_by_index(&mut env, index).now_or_never().unwrap();
        assert_eq!(result, Err(ResumeError::Unmonitored));
    }

    #[test]
    fn main_without_operands_resumes_current_job() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));
        let pgid = Pid::from_raw(100);
        let orphan_id = Pid::from_raw(200);
        let mut job = Job::new(pgid);
        let mut orphan = Job::new(orphan_id);
        job.job_controlled = true;
        orphan.job_controlled = true;
        let _ = env.jobs.add(orphan);
        let index = env.jobs.add(job);
        env.jobs.set_current_job(index).unwrap();
        let mut leader = Process::with_parent_and_group(system.process_id, pgid);
        let mut orphan = Process::with_parent_and_group(system.process_id, orphan_id);
        _ = leader.set_state(ProcessState::Stopped(Signal::SIGSTOP));
        _ = orphan.set_state(ProcessState::Stopped(Signal::SIGTTIN));
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
            ProcessState::Stopped(Signal::SIGTTIN),
        );
        // No error message should be printed on success.
        assert_stderr(&system.state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn main_without_operands_fails_if_there_is_no_current_job() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, crate::Result::from(ExitStatus::FAILURE));

        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("there is no job"), "{stderr:?}");
        });
    }

    #[test]
    fn main_with_operands_resumes_specified_jobs() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));
        let pgid1 = Pid::from_raw(100);
        let pgid2 = Pid::from_raw(200);
        let pgid3 = Pid::from_raw(300);
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
        _ = process1.set_state(ProcessState::Stopped(Signal::SIGSTOP));
        _ = process2.set_state(ProcessState::Stopped(Signal::SIGSTOP));
        _ = process3.set_state(ProcessState::Stopped(Signal::SIGSTOP));
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
            ProcessState::Stopped(Signal::SIGSTOP),
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
        let mut env = Env::with_system(Box::new(system.clone()));
        let pgid = Pid::from_raw(100);
        let mut job = Job::new(pgid);
        job.job_controlled = true;
        let index = env.jobs.add(job);
        env.jobs.set_current_job(index).unwrap();
        let mut leader = Process::with_parent_and_group(system.process_id, pgid);
        _ = leader.set_state(ProcessState::Stopped(Signal::SIGSTOP));
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
