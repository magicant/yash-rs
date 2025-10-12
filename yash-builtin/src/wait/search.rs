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
use std::borrow::Cow;
use yash_env::job::JobList;
use yash_env::job::id::FindError;
use yash_env::job::id::JobId;
use yash_env::job::id::ParseError;
use yash_env::semantics::Field;
use yash_syntax::source::pretty::Report;
use yash_syntax::source::pretty::ReportType;
use yash_syntax::source::pretty::Snippet;
#[allow(deprecated)]
use yash_syntax::source::pretty::{Annotation, AnnotationType, MessageBase};

/// Error returned when a job ID is ambiguous.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmbiguousJobId(Field);

impl AmbiguousJobId {
    /// Converts the error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = "ambiguous job ID".into();
        report.snippets = Snippet::with_primary_span(
            &self.0.origin,
            format!("job ID `{}` matches more than one job", self.0).into(),
        );
        report
    }
}

impl<'a> From<&'a AmbiguousJobId> for Report<'a> {
    #[inline]
    fn from(error: &'a AmbiguousJobId) -> Self {
        error.to_report()
    }
}

#[allow(deprecated)]
impl MessageBase for AmbiguousJobId {
    fn message_title(&self) -> Cow<'_, str> {
        "ambiguous job ID".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        Annotation::new(
            AnnotationType::Error,
            format!("job ID `{}` matches more than one job", self.0.value).into(),
            &self.0.origin,
        )
    }
}

/// Resolves a job ID to the index of the job in the job list.
///
/// If the job specification identifies a job, returns `Ok(Some(index))`.
/// If the job is not found, returns `Ok(None)`.
/// If the job ID is ambiguous, returns an error.
///
/// This function assumes that a `JobSpec::JobId` has a valid job ID that starts
/// with `%`. If the job ID lacks a leading `%`, this function panics.
pub fn resolve(jobs: &JobList, spec: JobSpec) -> Result<Option<usize>, AmbiguousJobId> {
    match spec {
        JobSpec::ProcessId(pid) => Ok(jobs.find_by_pid(pid)),

        JobSpec::JobId(field) => match JobId::try_from(field.value.as_str()) {
            Ok(id) => match id.find(jobs) {
                Ok(index) => Ok(Some(index)),
                Err(FindError::NotFound) => Ok(None),
                Err(FindError::Ambiguous) => Err(AmbiguousJobId(field)),
            },
            Err(ParseError) => panic!("job ID must start with `%`: {field:?}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::job::{Job, Pid};

    #[test]
    fn process_id_unique_match() {
        let mut jobs = JobList::new();
        let job1 = jobs.add(Job::new(Pid(123)));
        let job2 = jobs.add(Job::new(Pid(456)));

        let result1 = resolve(&jobs, JobSpec::ProcessId(Pid(123)));
        assert_eq!(result1, Ok(Some(job1)));
        let result2 = resolve(&jobs, JobSpec::ProcessId(Pid(456)));
        assert_eq!(result2, Ok(Some(job2)));
    }

    #[test]
    fn job_id_unique_match() {
        let mut jobs = JobList::new();
        let job1 = jobs.add(Job::new(Pid(123)));
        let job2 = jobs.add(Job::new(Pid(456)));

        let result1 = resolve(&jobs, JobSpec::JobId(Field::dummy("%1")));
        assert_eq!(result1, Ok(Some(job1)));
        let result2 = resolve(&jobs, JobSpec::JobId(Field::dummy("%2")));
        assert_eq!(result2, Ok(Some(job2)));
    }

    #[test]
    fn process_id_not_found() {
        let jobs = JobList::new();

        let result1 = resolve(&jobs, JobSpec::ProcessId(Pid(123)));
        assert_eq!(result1, Ok(None));
        let result2 = resolve(&jobs, JobSpec::ProcessId(Pid(456)));
        assert_eq!(result2, Ok(None));
    }

    #[test]
    fn job_id_not_found() {
        let jobs = JobList::new();

        let result1 = resolve(&jobs, JobSpec::JobId(Field::dummy("%1")));
        assert_eq!(result1, Ok(None));
        let result2 = resolve(&jobs, JobSpec::JobId(Field::dummy("%foo")));
        assert_eq!(result2, Ok(None));
    }

    #[test]
    fn job_id_ambiguous() {
        let mut jobs = JobList::new();
        let mut job1 = Job::new(Pid(123));
        job1.name = "sleep 1".into();
        jobs.add(job1);
        let mut job2 = Job::new(Pid(456));
        job2.name = "sleep 2".into();
        jobs.add(job2);

        let result = resolve(&jobs, JobSpec::JobId(Field::dummy("%sleep")));
        assert_eq!(result, Err(AmbiguousJobId(Field::dummy("%sleep"))));
    }
}
