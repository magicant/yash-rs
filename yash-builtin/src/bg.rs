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
//! This built-in can be used only when the [`Monitor`] option is set.
//!
//! It is an error if the specified job is not found.
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
//! # Implementation notes
//!
//! This implementation sends the `SIGCONT` signal even to jobs that are already
//! running.
//!
//! [`Monitor`]: yash_env::option::Monitor

use crate::common::report_error;
use crate::common::report_failure;
use crate::common::report_simple_failure;
use crate::common::syntax::parse_arguments;
use crate::common::syntax::Mode;
use crate::common::to_single_message;
use std::borrow::Cow;
use std::fmt::Display;
use thiserror::Error;
use yash_env::job::fmt::Marker;
use yash_env::job::fmt::Report;
use yash_env::job::id::parse;
use yash_env::job::id::FindError;
use yash_env::job::id::ParseError;
#[cfg(doc)]
use yash_env::job::JobSet;
use yash_env::job::Pid;
use yash_env::semantics::Field;
use yash_env::system::Errno;
use yash_env::trap::Signal;
use yash_env::Env;
use yash_env::System;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;
use yash_syntax::syntax::Fd;

/// Errors that may occur when resuming a job
#[derive(Clone, Debug, Error, Eq, PartialEq)]
enum ResumeError {
    #[error("target job is not job-controlled")]
    Unmonitored,
    #[error("system error: {0}")]
    SystemError(#[from] Errno),
}

/// Errors that may occur when processing an operand
#[derive(Clone, Debug, Error, Eq, PartialEq)]
enum OperandErrorKind {
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
    let job = env.jobs.get_mut(index).unwrap();
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

    let pgid = Pid::from_raw(-job.pid.as_raw());
    env.system.kill(pgid, Signal::SIGCONT.into())?;
    // TODO Don't kill if the job is already dead.

    // TODO env.jobs.set_current_job(index).ok();
    // TODO Update the job status but reset the status_changed flag.
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
            todo!("there is no job, error")
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

// TODO test
