// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Implementation of `Command::Send`
//!
//! [`execute`] calls [`send`] for each target and reports all errors.
//! [`send`] uses [`resolve_target`] to determine the argument to the
//! [`kill`](yash_env::System::kill) system call.

use super::Signal;
use crate::common::{report_failure, to_single_message};
use std::borrow::Cow;
use std::num::ParseIntError;
use thiserror::Error;
use yash_env::Env;
use yash_env::job::Pid;
use yash_env::job::id::parse_tail;
use yash_env::job::{JobList, id::FindError};
use yash_env::semantics::Field;
use yash_env::signal;
use yash_env::system::Errno;
use yash_env::system::System as _;
use yash_syntax::source::pretty::{Annotation, AnnotationType, MessageBase};

/// Error that may occur while [sending](send) a signal.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// The specified process (group) ID was not a valid integer.
    #[error(transparent)]
    ProcessId(#[from] ParseIntError),
    /// The specified job ID did not uniquely identify a job.
    #[error(transparent)]
    JobId(#[from] FindError),
    /// The target job is not controlled by the current shell environment.
    #[error("target job is not controlled by the current shell environment")]
    Unowned,
    /// The job ID specifies a job that is not job-controlled.
    #[error("target job is not job-controlled")]
    Unmonitored,
    /// The target job has finished.
    #[error("target job has finished")]
    Finished,
    /// An error occurred in the underlying system call.
    #[error(transparent)]
    System(#[from] Errno),
}

/// Resolves the specified target into a process (group) ID.
///
/// The target may be specified as a job ID, a process ID, or a process group
/// ID. In case of a process group ID, the value should be negative.
pub fn resolve_target(jobs: &JobList, target: &str) -> Result<Pid, Error> {
    if let Some(tail) = target.strip_prefix('%') {
        let job_id = parse_tail(tail);
        let index = job_id.find(jobs)?;
        let job = &jobs[index];
        if !job.is_owned {
            Err(Error::Unowned)
        } else if !job.job_controlled {
            Err(Error::Unmonitored)
        } else if !job.state.is_alive() {
            Err(Error::Finished)
        } else {
            Ok(-job.pid)
        }
    } else {
        Ok(Pid(target.parse()?))
    }
}

/// Sends the specified signal to the specified target.
pub async fn send(
    env: &mut Env,
    signal: Option<signal::Number>,
    target: &Field,
) -> Result<(), Error> {
    let pid = resolve_target(&env.jobs, &target.value)?;
    env.system.kill(pid, signal).await?;
    Ok(())
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("signal {signal} not supported on this system")]
struct UnsupportedSignal<'a> {
    signal: Signal,
    // TODO Consider: origin: &'a Location,
    origin: &'a Field,
}

impl MessageBase for UnsupportedSignal<'_> {
    fn message_title(&self) -> Cow<'_, str> {
        "unsupported signal".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        Annotation::new(
            AnnotationType::Error,
            self.to_string().into(),
            &self.origin.origin,
        )
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{target}: {error}")]
struct TargetError<'a> {
    target: &'a Field,
    error: Error,
}

impl MessageBase for TargetError<'_> {
    fn message_title(&self) -> Cow<'_, str> {
        "cannot send signal".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        Annotation::new(
            AnnotationType::Error,
            self.to_string().into(),
            &self.target.origin,
        )
    }
}

/// Executes the `Send` command.
///
/// This function sends the specified signal to the specified targets.
/// If an error occurs, it reports the error to the standard error and returns a
/// non-zero exit status.
///
/// `signal_origin` is the field that specified the signal. It is used to report
/// the error location if the signal is not supported on the current system. If
/// it is `None` and the `signal` is not supported, the function panics.
pub async fn execute(
    env: &mut Env,
    signal: Signal,
    signal_origin: Option<&Field>,
    targets: &[Field],
) -> crate::Result {
    let Ok(signal) = signal.to_number(&env.system) else {
        let origin = signal_origin.unwrap();
        let message = UnsupportedSignal { signal, origin };
        return report_failure(env, &message).await;
    };

    let mut errors = Vec::new();
    for target in targets {
        if let Err(error) = send(env, signal, target).await {
            errors.push(TargetError { target, error });
        }
    }

    if let Some(message) = to_single_message(&errors) {
        report_failure(env, message).await
    } else {
        crate::Result::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use yash_env::job::Job;
    use yash_env::job::ProcessState;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_env_test_helper::assert_stderr;

    #[test]
    fn resolve_target_process_ids() {
        let jobs = JobList::new();

        let result = resolve_target(&jobs, "123");
        assert_eq!(result, Ok(Pid(123)));

        let result = resolve_target(&jobs, "-456");
        assert_eq!(result, Ok(Pid(-456)));
    }

    #[test]
    fn resolve_target_job_id() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.job_controlled = true;
        job.is_owned = true;
        job.state = ProcessState::Running;
        job.name = "my job".into();
        jobs.add(job);

        let result = resolve_target(&jobs, "%my");
        assert_eq!(result, Ok(Pid(-123)));
    }

    #[test]
    fn resolve_target_job_find_error() {
        let jobs = JobList::new();
        let result = resolve_target(&jobs, "%my");
        assert_eq!(result, Err(Error::JobId(FindError::NotFound)));
    }

    #[test]
    fn resolve_target_unowned() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.job_controlled = true;
        job.is_owned = false;
        job.state = ProcessState::Running;
        job.name = "my job".into();
        jobs.add(job);

        let result = resolve_target(&jobs, "%my");
        assert_eq!(result, Err(Error::Unowned));
    }

    #[test]
    fn resolve_target_unmonitored() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.job_controlled = false;
        job.is_owned = true;
        job.state = ProcessState::Running;
        job.name = "my job".into();
        jobs.add(job);

        let result = resolve_target(&jobs, "%my");
        assert_eq!(result, Err(Error::Unmonitored));
    }

    #[test]
    fn resolve_target_finished() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.job_controlled = true;
        job.is_owned = true;
        job.state = ProcessState::exited(0);
        job.name = "my job".into();
        jobs.add(job);

        let result = resolve_target(&jobs, "%my");
        assert_eq!(result, Err(Error::Finished));
    }

    #[test]
    fn resolve_target_invalid_string() {
        let jobs = JobList::new();
        let result = resolve_target(&jobs, "abc");
        assert_matches!(result, Err(Error::ProcessId(_)));
    }

    #[test]
    fn execute_unsupported_signal() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let result = execute(&mut env, Signal::Number(-1), Some(&Field::dummy("-1")), &[])
            .now_or_never()
            .unwrap();
        assert_eq!(result, crate::Result::from(ExitStatus::FAILURE));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }
}
