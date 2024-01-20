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

//! Command line argument parser for the wait built-in

use super::{Command, JobSpec};
use std::borrow::Cow;
use std::num::ParseIntError;
use thiserror::Error;
use yash_env::job::Pid;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_syntax::source::pretty::{Annotation, AnnotationType, MessageBase};

use crate::common::syntax::{parse_arguments, Mode, ParseError};

/// Errors that may occur while parsing command line arguments
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum Error {
    /// Error in generic argument parsing
    #[error(transparent)]
    CommonError(#[from] ParseError<'static>),

    /// An operand does not start with `%` and is not a decimal integer.
    #[error("{0}: {1}")]
    ParseInt(Field, ParseIntError),

    /// An operand is a negative decimal integer.
    #[error("{0}: non-positive process ID")]
    NonPositive(Field),
}

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        "invalid job specification".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        let field = match self {
            Self::CommonError(e) => return e.main_annotation(),
            Self::ParseInt(field, _) | Self::NonPositive(field) => field,
        };
        Annotation::new(
            AnnotationType::Error,
            self.to_string().into(),
            &field.origin,
        )
    }
}

impl TryFrom<Field> for JobSpec {
    type Error = Error;

    fn try_from(field: Field) -> Result<Self, Error> {
        if field.value.starts_with('%') {
            return Ok(Self::JobId(field));
        }
        match field.value.parse() {
            Ok(int) if int >= 0 => Ok(Self::ProcessId(Pid(int))),
            Ok(_) => Err(Error::NonPositive(field)),
            Err(error) => Err(Error::ParseInt(field, error)),
        }
    }
}

/// Parses command line arguments for the wait built-in.
pub fn parse(env: &Env, args: Vec<Field>) -> Result<Command, Error> {
    let (_, operands) = parse_arguments(&[], Mode::with_env(env), args)?;
    let jobs = operands
        .into_iter()
        .map(JobSpec::try_from)
        .collect::<Result<Vec<JobSpec>, Error>>()?;
    Ok(Command { jobs })
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::num::IntErrorKind;

    #[test]
    fn non_negative_process_ids() {
        let result = JobSpec::try_from(Field::dummy("123"));
        assert_eq!(result, Ok(JobSpec::ProcessId(Pid(123))));

        let result = JobSpec::try_from(Field::dummy("0"));
        assert_eq!(result, Ok(JobSpec::ProcessId(Pid(0))));
    }

    #[test]
    fn negative_process_ids() {
        let result = JobSpec::try_from(Field::dummy("-1"));
        assert_eq!(result, Err(Error::NonPositive(Field::dummy("-1"))));

        let result = JobSpec::try_from(Field::dummy("-121"));
        assert_eq!(result, Err(Error::NonPositive(Field::dummy("-121"))));
    }

    #[test]
    fn unparsable_process_ids() {
        let result = JobSpec::try_from(Field::dummy("abc"));
        assert_matches!(result, Err(Error::ParseInt(field, error)) => {
            assert_eq!(field, Field::dummy("abc"));
            assert_eq!(error.kind(), &IntErrorKind::InvalidDigit);
        });

        let result = JobSpec::try_from(Field::dummy(""));
        assert_matches!(result, Err(Error::ParseInt(field, error)) => {
            assert_eq!(field, Field::dummy(""));
            assert_eq!(error.kind(), &IntErrorKind::Empty);
        });
    }

    #[test]
    fn job_ids() {
        let result = JobSpec::try_from(Field::dummy("%abc"));
        assert_eq!(result, Ok(JobSpec::JobId(Field::dummy("%abc"))));

        let result = JobSpec::try_from(Field::dummy("%"));
        assert_eq!(result, Ok(JobSpec::JobId(Field::dummy("%"))));
    }
}
