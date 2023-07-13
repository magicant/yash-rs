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

//! Job ID parsing
//!
//! This module provides functionalities to parse job IDs and find jobs by using
//! them. A job ID is a string that identifies a job contained in a job set. The
//! string can take several forms:
//!
//! - Job IDs `%`, `%%`, and `%+` denote the [current job](JobSet::current_job).
//! - Job ID `%-` specifies the [previous job](JobSet::previous_job).
//! - A job ID of the form `%n` (where `n` is a positive integer) refers to the
//!   job with the specified job number, that is, the job with index `n-1`.
//! - A job ID of the form `%name` (where `name` is a string not starting with a
//!   `?`) represents a job whose name begins with `name`.
//! - A job ID of the form `%?name` (where `name` is a string) selects a job
//!   whose name contains `name`.
//!
//! A job ID is ambiguous if a name in the ID matches more than one job name.
//! Job IDs not starting with a '%' may be accepted depending on on the context,
//! which is not handled in this module.
//!
//! You can parse a job ID with [`parse`] or [`parse_tail`] and get a [`JobId`]
//! as a result.

use super::Job;
use super::JobSet;
use std::fmt::Display;
use std::fmt::Formatter;
use std::num::NonZeroUsize;
use thiserror::Error;

/// Result of parsing a job ID
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobId<'a> {
    /// The current job (`%`, `%%`, or `%+`)
    CurrentJob,
    /// The previous job (`%-`)
    PreviousJob,
    /// Job with a specific job number (`%n`)
    JobNumber(NonZeroUsize),
    /// Job with a name starting with a specific string (`%name`)
    NamePrefix(&'a str),
    /// Job with a name containing a specific string (`%?name`)
    NameSubstring(&'a str),
}

/// Defines `CurrentJob` as the default job ID.
impl Default for JobId<'_> {
    fn default() -> Self {
        JobId::CurrentJob
    }
}

/// Converts a job ID to the original string form.
///
/// `CurrentJob` will be `"%+"` rather than `"%%"` or `"%"`.
impl Display for JobId<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match *self {
            JobId::CurrentJob => "%+".fmt(f),
            JobId::PreviousJob => "%-".fmt(f),
            JobId::JobNumber(number) => write!(f, "%{number}"),
            JobId::NamePrefix(prefix) => write!(f, "%{prefix}"),
            JobId::NameSubstring(substring) => write!(f, "%?{substring}"),
        }
    }
}

/// Error that may occur in job ID [parsing](parse)
#[derive(Clone, Copy, Debug, Eq, Error, Hash, PartialEq)]
#[error("a job ID must start with a '%'")]
pub struct ParseError;

/// Parses a job ID excluding the initial `%`.
///
/// This function requires a job ID string that does not contain the initial `%`
/// character. See also [`parse`].
///
/// ```
/// # use std::num::NonZeroUsize;
/// # use yash_env::job::id::{JobId, parse_tail};
/// assert_eq!(parse_tail(""), JobId::CurrentJob);
/// assert_eq!(parse_tail("%"), JobId::CurrentJob);
/// assert_eq!(parse_tail("+"), JobId::CurrentJob);
/// assert_eq!(parse_tail("-"), JobId::PreviousJob);
/// assert_eq!(parse_tail("1"), JobId::JobNumber(NonZeroUsize::new(1).unwrap()));
/// assert_eq!(parse_tail("foo"), JobId::NamePrefix("foo"));
/// assert_eq!(parse_tail("?foo"), JobId::NameSubstring("foo"));
/// ```
pub fn parse_tail(tail: &str) -> JobId {
    match tail {
        "" | "%" | "+" => JobId::CurrentJob,
        "-" => JobId::PreviousJob,
        _ => match tail.strip_prefix('?') {
            Some(substring) => JobId::NameSubstring(substring),
            None => match tail.parse::<NonZeroUsize>() {
                Ok(number) => JobId::JobNumber(number),
                Err(_) => JobId::NamePrefix(tail),
            },
        },
    }
}

/// Parses a job ID.
///
/// This function requires a string starting with a `%`. If the string lacks the
/// leading `%`, the result is a `ParseError`. See also [`parse_tail`].
///
/// ```
/// # use std::num::NonZeroUsize;
/// # use yash_env::job::id::{JobId, ParseError, parse};
/// assert_eq!(parse(""), Err(ParseError));
/// assert_eq!(parse("%"), Ok(JobId::CurrentJob));
/// assert_eq!(parse("%%"), Ok(JobId::CurrentJob));
/// assert_eq!(parse("%foo"), Ok(JobId::NamePrefix("foo")));
/// assert_eq!(parse("%?foo"), Ok(JobId::NameSubstring("foo")));
/// assert_eq!(parse("foo"), Err(ParseError));
/// ```
pub fn parse(job_id: &str) -> Result<JobId, ParseError> {
    match job_id.strip_prefix('%') {
        Some(tail) => Ok(parse_tail(tail)),
        None => Err(ParseError),
    }
}

/// Parses a job ID string.
impl<'a> TryFrom<&'a str> for JobId<'a> {
    type Error = ParseError;
    #[inline(always)]
    fn try_from(s: &'a str) -> Result<JobId<'a>, ParseError> {
        parse(s)
    }
}

/// Error that may occur in [`JobId::find`]
#[derive(Clone, Copy, Debug, Eq, Error, Hash, PartialEq)]
pub enum FindError {
    /// There is no job that matches the job ID.
    #[error("job not found")]
    NotFound,

    /// There are more than one job that matches the job ID.
    #[error("ambiguous job")]
    Ambiguous,
}

impl JobId<'_> {
    /// Returns the index of a job matching the job ID.
    ///
    /// This function succeeds if there is exactly one job that matches the job
    /// ID. Otherwise, the result is a `FindError`.
    pub fn find(&self, jobs: &JobSet) -> Result<usize, FindError> {
        fn find_one(
            jobs: &JobSet,
            pred: &mut dyn FnMut(&(usize, &Job)) -> bool,
        ) -> Result<usize, FindError> {
            let mut i = jobs.iter().filter(pred).map(|(index, _)| index);
            let index = i.next().ok_or(FindError::NotFound)?;
            match i.next() {
                Some(_) => Err(FindError::Ambiguous),
                None => Ok(index),
            }
        }

        match *self {
            JobId::CurrentJob => jobs.current_job().ok_or(FindError::NotFound),
            JobId::PreviousJob => jobs.previous_job().ok_or(FindError::NotFound),
            JobId::JobNumber(number) => {
                let index = number.get() - 1;
                match jobs.get(index) {
                    Some(_) => Ok(index),
                    None => Err(FindError::NotFound),
                }
            }
            JobId::NamePrefix(prefix) => {
                find_one(jobs, &mut |&(_, job)| job.name.starts_with(prefix))
            }
            JobId::NameSubstring(substring) => {
                find_one(jobs, &mut |&(_, job)| job.name.contains(substring))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Pid;
    use super::*;

    #[test]
    fn job_id_display() {
        assert_eq!(JobId::CurrentJob.to_string(), "%+");
        assert_eq!(JobId::PreviousJob.to_string(), "%-");
        assert_eq!(
            JobId::JobNumber(NonZeroUsize::new(42).unwrap()).to_string(),
            "%42"
        );
        assert_eq!(JobId::NamePrefix("foo").to_string(), "%foo");
        assert_eq!(JobId::NameSubstring("bar").to_string(), "%?bar");
    }

    fn sample_job_set() -> JobSet {
        let mut set = JobSet::default();

        let mut job = Job::new(Pid::from_raw(10));
        job.name = "first job".to_string();
        set.add(job);

        let mut job = Job::new(Pid::from_raw(11));
        job.name = "job 2".to_string();
        set.add(job);

        let mut job = Job::new(Pid::from_raw(12));
        job.name = "last one".to_string();
        set.add(job);

        set
    }

    #[test]
    fn find_unique_current_job() {
        let set = sample_job_set();
        let job_id = JobId::CurrentJob;
        let current_job_index = set.current_job().unwrap();
        assert_eq!(job_id.find(&set), Ok(current_job_index));
    }

    #[test]
    fn find_unique_previous_job() {
        let set = sample_job_set();
        let job_id = JobId::PreviousJob;
        let previous_job_index = set.previous_job().unwrap();
        assert_eq!(job_id.find(&set), Ok(previous_job_index));
    }

    #[test]
    fn find_unique_job_by_job_number() {
        let set = sample_job_set();
        let job_id = JobId::JobNumber(NonZeroUsize::new(1).unwrap());
        assert_eq!(job_id.find(&set), Ok(0));
        let job_id = JobId::JobNumber(NonZeroUsize::new(2).unwrap());
        assert_eq!(job_id.find(&set), Ok(1));
        let job_id = JobId::JobNumber(NonZeroUsize::new(3).unwrap());
        assert_eq!(job_id.find(&set), Ok(2));
    }

    #[test]
    fn find_unique_job_by_name_prefix() {
        let set = sample_job_set();
        let job_id = JobId::NamePrefix("first");
        assert_eq!(job_id.find(&set), Ok(0));

        let job_id = JobId::NamePrefix("job");
        assert_eq!(job_id.find(&set), Ok(1));
    }

    #[test]
    fn find_unique_job_by_name_substring() {
        let set = sample_job_set();
        let job_id = JobId::NameSubstring("one");
        assert_eq!(job_id.find(&set), Ok(2));
    }

    #[test]
    fn find_no_current_job() {
        let set = JobSet::default();
        let job_id = JobId::CurrentJob;
        assert_eq!(job_id.find(&set), Err(FindError::NotFound));
    }

    #[test]
    fn find_no_previous_job() {
        let set = JobSet::default();
        let job_id = JobId::PreviousJob;
        assert_eq!(job_id.find(&set), Err(FindError::NotFound));
    }

    #[test]
    fn find_no_job_for_job_number() {
        let set = JobSet::default();
        let job_id = JobId::JobNumber(NonZeroUsize::new(1).unwrap());
        assert_eq!(job_id.find(&set), Err(FindError::NotFound));
        let job_id = JobId::JobNumber(NonZeroUsize::new(2).unwrap());
        assert_eq!(job_id.find(&set), Err(FindError::NotFound));
        let job_id = JobId::JobNumber(NonZeroUsize::new(3).unwrap());
        assert_eq!(job_id.find(&set), Err(FindError::NotFound));
    }

    #[test]
    fn find_no_job_for_prefix() {
        let set = JobSet::default();
        let job_id = JobId::NamePrefix("first");
        assert_eq!(job_id.find(&set), Err(FindError::NotFound));

        let set = sample_job_set();
        let job_id = JobId::NamePrefix("one");
        assert_eq!(job_id.find(&set), Err(FindError::NotFound));
    }

    #[test]
    fn find_no_job_for_substring() {
        let set = JobSet::default();
        let job_id = JobId::NameSubstring("foo");
        assert_eq!(job_id.find(&set), Err(FindError::NotFound));
    }

    #[test]
    fn find_ambiguous_prefix() {
        let mut set = sample_job_set();

        let mut job = Job::new(Pid::from_raw(20));
        job.name = "job 3".to_string();
        set.add(job);

        let job_id = JobId::NamePrefix("job");
        assert_eq!(job_id.find(&set), Err(FindError::Ambiguous));
    }

    #[test]
    fn find_ambiguous_substring() {
        let set = sample_job_set();
        let job_id = JobId::NameSubstring("job");
        assert_eq!(job_id.find(&set), Err(FindError::Ambiguous));
    }
}
