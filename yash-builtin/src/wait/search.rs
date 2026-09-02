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

//! Resolving job specifications

use super::JobSpec;
use crate::common::report::job;
use thiserror::Error;
use yash_env::job::JobList;
use yash_env::job::id::FindError;
use yash_env::job::id::ParseError;
use yash_env::job::id::parse_tail;
use yash_env::option::State;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::source::pretty::Report;
use yash_env::source::pretty::ReportType;
use yash_env::source::pretty::Snippet;

/// Error that may occur while [resolving](resolve) a job specification
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// The job ID is a lone `%`, which POSIX does not specify.
    #[error("{}: {}", .0.value, ParseError::LonePercent)]
    NonPortableJobId(Field),

    /// The job ID matches more than one job.
    #[error("job ID `{0}` matches more than one job")]
    AmbiguousJobId(Field),
}

impl Error {
    /// Converts the error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let (title, field) = match self {
            Self::NonPortableJobId(field) => ("non-portable job ID", field),
            Self::AmbiguousJobId(field) => ("ambiguous job ID", field),
        };
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = title.into();
        report.snippets = Snippet::with_primary_span(&field.origin, self.to_string().into());
        if let Self::NonPortableJobId(_) = self {
            report.footnotes = job::non_portable_footnotes(ParseError::LonePercent);
        }
        report
    }

    /// Returns the exit status the built-in should return for this error.
    ///
    /// A job ID the built-in cannot parse is a command argument error, which it
    /// reports with [`ExitStatus::ERROR`]. A job ID that does not identify a
    /// single job is a runtime failure, reported with [`ExitStatus::FAILURE`].
    #[must_use]
    pub fn exit_status(&self) -> ExitStatus {
        match self {
            Self::NonPortableJobId(_) => ExitStatus::ERROR,
            Self::AmbiguousJobId(_) => ExitStatus::FAILURE,
        }
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

/// Resolves a job ID to the index of the job in the job list.
///
/// If the job specification identifies a job, returns `Ok(Some(index))`.
/// If the job is not found, returns `Ok(None)`.
///
/// This function parses the job ID contained in a `JobSpec::JobId`. If
/// `portable` is [`State::On`], a job ID that POSIX does not specify is
/// rejected.
pub fn resolve(jobs: &JobList, spec: JobSpec, portable: State) -> Result<Option<usize>, Error> {
    match spec {
        JobSpec::ProcessId(pid) => Ok(jobs.find_by_pid(pid)),

        JobSpec::JobId(field) => {
            // The `syntax` module produces this variant only for an operand
            // that starts with a `%`, so the leading `%` may just be dropped.
            let tail = field.value.strip_prefix('%').unwrap_or(&field.value);
            match parse_tail(tail, portable) {
                Ok(id) => match id.find(jobs) {
                    Ok(index) => Ok(Some(index)),
                    Err(FindError::NotFound) => Ok(None),
                    Err(FindError::Ambiguous) => Err(Error::AmbiguousJobId(field)),
                },
                Err(_) => Err(Error::NonPortableJobId(field)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use yash_env::job::{Job, Pid};

    #[test]
    fn process_id_unique_match() {
        let mut jobs = JobList::new();
        let job1 = jobs.insert(Job::new(Pid(123)));
        let job2 = jobs.insert(Job::new(Pid(456)));

        let result1 = resolve(&jobs, JobSpec::ProcessId(Pid(123)), State::Off);
        assert_eq!(result1, Ok(Some(job1)));
        let result2 = resolve(&jobs, JobSpec::ProcessId(Pid(456)), State::Off);
        assert_eq!(result2, Ok(Some(job2)));
    }

    #[test]
    fn job_id_unique_match() {
        let mut jobs = JobList::new();
        let job1 = jobs.insert(Job::new(Pid(123)));
        let job2 = jobs.insert(Job::new(Pid(456)));

        let result1 = resolve(&jobs, JobSpec::JobId(Field::dummy("%1")), State::Off);
        assert_eq!(result1, Ok(Some(job1)));
        let result2 = resolve(&jobs, JobSpec::JobId(Field::dummy("%2")), State::Off);
        assert_eq!(result2, Ok(Some(job2)));
    }

    #[test]
    fn process_id_not_found() {
        let jobs = JobList::new();

        let result1 = resolve(&jobs, JobSpec::ProcessId(Pid(123)), State::Off);
        assert_eq!(result1, Ok(None));
        let result2 = resolve(&jobs, JobSpec::ProcessId(Pid(456)), State::Off);
        assert_eq!(result2, Ok(None));
    }

    #[test]
    fn job_id_not_found() {
        let jobs = JobList::new();

        let result1 = resolve(&jobs, JobSpec::JobId(Field::dummy("%1")), State::Off);
        assert_eq!(result1, Ok(None));
        let result2 = resolve(&jobs, JobSpec::JobId(Field::dummy("%foo")), State::Off);
        assert_eq!(result2, Ok(None));
    }

    #[test]
    fn job_id_ambiguous() {
        let mut jobs = JobList::new();
        let mut job1 = Job::new(Pid(123));
        job1.name = "sleep 1".into();
        jobs.insert(job1);
        let mut job2 = Job::new(Pid(456));
        job2.name = "sleep 2".into();
        jobs.insert(job2);

        let result = resolve(&jobs, JobSpec::JobId(Field::dummy("%sleep")), State::Off);
        assert_eq!(result, Err(Error::AmbiguousJobId(Field::dummy("%sleep"))));
    }

    #[test]
    fn lone_percent_is_non_portable() {
        let mut jobs = JobList::new();
        jobs.insert(Job::new(Pid(123)));

        let result = resolve(&jobs, JobSpec::JobId(Field::dummy("%")), State::On);

        assert_eq!(result, Err(Error::NonPortableJobId(Field::dummy("%"))),);
    }

    #[test]
    fn portable_job_ids_are_accepted_under_the_portable_option() {
        let mut jobs = JobList::new();
        let job = jobs.insert(Job::new(Pid(123)));

        for value in ["%%", "%+", "%1"] {
            let result = resolve(&jobs, JobSpec::JobId(Field::dummy(value)), State::On);
            assert_eq!(result, Ok(Some(job)), "{value}");
        }
    }

    #[test]
    fn non_portable_job_id_report_has_portable_footnote() {
        let error = Error::NonPortableJobId(Field::dummy("%"));

        let report = error.to_report();

        assert_eq!(report.title, "non-portable job ID");
        assert_matches!(&report.footnotes[..], [suggestion, portable] => {
            assert_eq!(suggestion.label, "use `%%` or `%+` instead");
            assert_eq!(
                portable.label,
                "this error is reported because the `portable` shell option is enabled",
            );
        });
    }

    #[test]
    fn exit_status_is_error_only_for_non_portable_job_id() {
        let field = Field::dummy("%");
        assert_eq!(
            Error::NonPortableJobId(field.clone()).exit_status(),
            ExitStatus::ERROR,
        );
        assert_eq!(
            Error::AmbiguousJobId(field).exit_status(),
            ExitStatus::FAILURE,
        );
    }
}
