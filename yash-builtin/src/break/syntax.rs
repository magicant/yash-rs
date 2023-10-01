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

//! Command line argument parser for the break/continue built-in

use crate::common::syntax::parse_arguments;
use crate::common::syntax::Mode;
use std::borrow::Cow;
use std::num::NonZeroUsize;
use std::num::ParseIntError;
use thiserror::Error;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;

/// Error in parsing command line arguments
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common parser.
    #[error(transparent)]
    CommonError(#[from] crate::common::syntax::ParseError<'static>),

    /// More than one operand is given.
    #[error("too many operands")]
    TooManyOperands(Vec<Field>),

    /// The operand is not a valid positive integer.
    #[error("invalid numeric operand")]
    InvalidNumber(Field, ParseIntError),
}

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        self.to_string().into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        use Error::*;
        match self {
            CommonError(e) => e.main_annotation(),
            TooManyOperands(operands) => Annotation::new(
                AnnotationType::Error,
                format!("{}: redundant operand", operands[1].value).into(),
                &operands[1].origin,
            ),
            InvalidNumber(operand, e) => Annotation::new(
                AnnotationType::Error,
                format!("{}: {}", operand.value, e).into(),
                &operand.origin,
            ),
        }
    }
}

/// Result of parsing command line arguments
///
/// If successful, the result is the number of levels to break.
pub type Result = std::result::Result<NonZeroUsize, Error>;

/// Parses command line arguments for the break/continue built-in.
pub fn parse(env: &Env, args: Vec<Field>) -> Result {
    // TODO: POSIX does not require the break/continue built-in to support XBD
    // Utility Syntax Guidelines. That means the built-in does not have to
    // recognize the "--" separator. We should reject the separator in the
    // POSIXly-correct mode.

    let (_options, mut operands) = parse_arguments(&[], Mode::with_env(env), args)?;

    if operands.len() > 1 {
        return Err(Error::TooManyOperands(operands));
    }

    match operands.pop() {
        None => Ok(NonZeroUsize::new(1).unwrap()),

        Some(field) => field
            .value
            .parse()
            .map_err(|e| Error::InvalidNumber(field, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::num::IntErrorKind;

    #[test]
    fn default_count() {
        let env = Env::new_virtual();
        let result = parse(&env, vec![]);
        assert_eq!(result, Ok(NonZeroUsize::new(1).unwrap()));
    }

    #[test]
    fn valid_counts() {
        let env = Env::new_virtual();
        let args = Field::dummies(["1"]);
        let result = parse(&env, args);
        assert_eq!(result, Ok(NonZeroUsize::new(1).unwrap()));

        let args = Field::dummies(["2"]);
        let result = parse(&env, args);
        assert_eq!(result, Ok(NonZeroUsize::new(2).unwrap()));
    }

    #[test]
    fn too_many_operands() {
        let env = Env::new_virtual();
        let args = Field::dummies(["1", "2"]);
        let result = parse(&env, args.clone());
        assert_eq!(result, Err(Error::TooManyOperands(args)));
    }

    #[test]
    fn non_positive_integer() {
        let env = Env::new_virtual();
        let arg = Field::dummy("0");
        let result = parse(&env, vec![arg.clone()]);
        assert_matches!(result, Err(Error::InvalidNumber(field, error)) => {
            assert_eq!(field, arg);
            assert_eq!(error.kind(), &IntErrorKind::Zero);
        });
    }
}
