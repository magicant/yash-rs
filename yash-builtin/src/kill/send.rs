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
//! [`kill`](SendSignal::kill) system call.

use crate::common::report::job;
use crate::common::report::{merge_reports, report, report_failure};
use std::num::{NonZero, ParseIntError};
use thiserror::Error;
use yash_env::Env;
use yash_env::job::Pid;
use yash_env::job::id::{ParseError, parse_tail};
use yash_env::job::{JobList, id::FindError};
use yash_env::option::Option::Portable;
use yash_env::option::State;
use yash_env::semantics::{ExitStatus, Field};
use yash_env::signal::{Number, RawNumber};
use yash_env::source::pretty::{Report, ReportType, Snippet};
use yash_env::system::concurrency::WriteAll;
use yash_env::system::{Errno, Isatty, SendSignal, Signals};

/// Error that may occur while [sending](send) a signal.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The specified process (group) ID was not a valid integer.
    #[error(transparent)]
    ProcessId(#[from] ParseIntError),
    /// The specified job ID was not a portable job ID.
    #[error(transparent)]
    JobIdSyntax(#[from] ParseError),
    /// The specified job ID did not uniquely identify a job.
    #[error(transparent)]
    JobIdSearch(#[from] FindError),
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
///
/// If `portable` is [`State::On`], a non-portable job ID is rejected with
/// [`Error::JobIdSyntax`].
pub fn resolve_target(jobs: &JobList, target: &str, portable: State) -> Result<Pid, Error> {
    if let Some(tail) = target.strip_prefix('%') {
        let job_id = parse_tail(tail, portable)?;
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
pub async fn send<S: SendSignal>(
    env: &mut Env<S>,
    signal: Option<Number>,
    target: &Field,
) -> Result<(), Error> {
    let pid = resolve_target(&env.jobs, &target.value, env.options.get(Portable))?;
    env.system.kill(pid, signal).await?;
    Ok(())
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("signal {signal} not supported on this system")]
struct UnsupportedSignal<'a> {
    signal: RawNumber,
    // TODO Consider: origin: &'a Location,
    origin: &'a Field,
}

impl UnsupportedSignal<'_> {
    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = "unsupported signal".into();
        report.snippets = Snippet::with_primary_span(&self.origin.origin, self.to_string().into());
        report
    }
}

impl<'a> From<&'a UnsupportedSignal<'a>> for Report<'a> {
    #[inline]
    fn from(error: &'a UnsupportedSignal<'a>) -> Self {
        error.to_report()
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{target}: {error}")]
struct TargetError<'a> {
    target: &'a Field,
    error: Error,
}

impl TargetError<'_> {
    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = "cannot send signal".into();
        report.snippets = Snippet::with_primary_span(
            &self.target.origin,
            format!("{}: {}", self.target.value, self.error).into(),
        );
        // A job ID is rejected here only when the `portable` option is on:
        // `resolve_target` supplies the leading `%` itself.
        if let Error::JobIdSyntax(error) = self.error {
            report.footnotes = job::non_portable_footnotes(error);
        }
        report
    }

    /// Returns the exit status the built-in should return for this error.
    ///
    /// A target the built-in cannot parse is a command argument error, which it
    /// reports with [`ExitStatus::ERROR`]. A target that names a job the
    /// built-in cannot signal is a runtime failure, reported with
    /// [`ExitStatus::FAILURE`].
    #[must_use]
    fn exit_status(&self) -> ExitStatus {
        match self.error {
            Error::ProcessId(_) | Error::JobIdSyntax(_) => ExitStatus::ERROR,
            Error::JobIdSearch(_)
            | Error::Unowned
            | Error::Unmonitored
            | Error::Finished
            | Error::System(_) => ExitStatus::FAILURE,
        }
    }
}

impl<'a> From<&'a TargetError<'a>> for Report<'a> {
    #[inline]
    fn from(error: &'a TargetError<'a>) -> Self {
        error.to_report()
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
pub async fn execute<S>(
    env: &mut Env<S>,
    signal: RawNumber,
    signal_origin: Option<&Field>,
    targets: &[Field],
) -> crate::Result
where
    S: Isatty + SendSignal + Signals + WriteAll,
{
    let signal_number = NonZero::new(signal).map(Number::from_raw_unchecked);

    let mut errors = Vec::new();
    for target in targets {
        match send(env, signal_number, target).await {
            Ok(()) => (),
            Err(Error::System(Errno::EINVAL)) => {
                let origin = signal_origin.unwrap();
                let report = UnsupportedSignal { signal, origin };
                return report_failure(env, &report).await;
            }
            Err(error) => errors.push(TargetError { target, error }),
        }
    }

    let Some(exit_status) = errors.iter().map(TargetError::exit_status).max() else {
        return crate::Result::default();
    };
    let merged = merge_reports(&errors).unwrap();
    report(env, merged, exit_status).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::assert_matches;
    use std::rc::Rc;
    use yash_env::job::Job;
    use yash_env::job::ProcessState;
    use yash_env::option::Option::Portable;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::Concurrent;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_env::test_helper::assert_stderr;

    #[test]
    fn resolve_target_process_ids() {
        let jobs = JobList::new();

        let result = resolve_target(&jobs, "123", State::Off);
        assert_eq!(result, Ok(Pid(123)));

        let result = resolve_target(&jobs, "-456", State::Off);
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
        jobs.insert(job);

        let result = resolve_target(&jobs, "%my", State::Off);
        assert_eq!(result, Ok(Pid(-123)));
    }

    #[test]
    fn resolve_target_job_find_error() {
        let jobs = JobList::new();
        let result = resolve_target(&jobs, "%my", State::Off);
        assert_eq!(result, Err(Error::JobIdSearch(FindError::NotFound)));
    }

    #[test]
    fn resolve_target_unowned() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.job_controlled = true;
        job.is_owned = false;
        job.state = ProcessState::Running;
        job.name = "my job".into();
        jobs.insert(job);

        let result = resolve_target(&jobs, "%my", State::Off);
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
        jobs.insert(job);

        let result = resolve_target(&jobs, "%my", State::Off);
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
        jobs.insert(job);

        let result = resolve_target(&jobs, "%my", State::Off);
        assert_eq!(result, Err(Error::Finished));
    }

    #[test]
    fn resolve_target_rejects_lone_percent_when_portable() {
        let jobs = JobList::new();
        let result = resolve_target(&jobs, "%", State::On);
        assert_eq!(result, Err(Error::JobIdSyntax(ParseError::LonePercent)));
    }

    #[test]
    fn non_portable_job_id_report_has_portable_footnote() {
        let target = Field::dummy("%");
        let error = TargetError {
            target: &target,
            error: Error::JobIdSyntax(ParseError::LonePercent),
        };

        let report = error.to_report();

        assert_matches::assert_matches!(&report.footnotes[..], [suggestion, portable] => {
            assert_eq!(suggestion.label, "use `%%` or `%+` instead");
            assert_eq!(
                portable.label,
                "this error is reported because the `portable` shell option is enabled",
            );
        });
    }

    #[test]
    fn resolve_target_invalid_string() {
        let jobs = JobList::new();
        let result = resolve_target(&jobs, "abc", State::Off);
        assert_matches!(result, Err(Error::ProcessId(_)));
    }

    #[test]
    fn execute_unsupported_signal() {
        let system = VirtualSystem::new();
        let pid = system.process_id.to_string();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        let result = execute(
            &mut env,
            -1,
            Some(&Field::dummy("-1")),
            &Field::dummies([pid]),
        )
        .now_or_never()
        .unwrap();
        assert_eq!(result, crate::Result::from(ExitStatus::FAILURE));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn execute_non_portable_job_id_returns_error_exit_status() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Portable, State::On);

        let result = execute(&mut env, 0, None, &Field::dummies(["%"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::from(ExitStatus::ERROR));
        assert_stderr(&state, |stderr| {
            assert!(
                stderr.contains("a lone '%' is not a portable job ID"),
                "stderr = {stderr:?}",
            );
        });
    }

    #[test]
    fn execute_invalid_target_returns_error_exit_status() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));

        let result = execute(&mut env, 0, None, &Field::dummies(["foo"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::from(ExitStatus::ERROR));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn execute_repeats_the_portable_footnote_only_once() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Portable, State::On);

        let result = execute(&mut env, 0, None, &Field::dummies(["%", "%"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::from(ExitStatus::ERROR));
        assert_stderr(&state, |stderr| {
            let note = "this error is reported because the `portable` shell option is enabled";
            assert_eq!(stderr.matches(note).count(), 1, "stderr = {stderr:?}");
        });
    }
}
