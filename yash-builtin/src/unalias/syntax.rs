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

//! Command line argument parsing for the `unalias` built-in

use super::Command;
use crate::common::syntax::Mode;
use crate::common::syntax::OptionSpec;
use crate::common::syntax::parse_arguments;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::source::Location;
#[allow(deprecated)]
use yash_env::source::pretty::{Annotation, AnnotationType, Message};
use yash_env::source::pretty::{Report, ReportType, Snippet, Span, SpanRole, add_span};

/// List of all options supported by the `unalias` built-in
pub const OPTION_SPECS: &[OptionSpec] = &[OptionSpec::new().short('a')];

/// Errors that can occur while parsing command line arguments
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum Error {
    /// An error occurred while parsing arguments.
    #[error(transparent)]
    CommonError(#[from] crate::common::syntax::ParseError<'static>),

    /// The `-a` option was specified with other operands.
    #[error("`-a` cannot be used with operands")]
    ConflictingOptionAndOperand {
        option_location: Location,
        operand_location: Location,
    },

    /// No option or operand was specified.
    #[error("no option or operand specified")]
    MissingArgument,
}

impl Error {
    /// Converts the error into a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let (title, snippets) = match self {
            Self::CommonError(e) => return e.to_report(),

            Self::ConflictingOptionAndOperand {
                option_location,
                operand_location,
            } => ("`-a` cannot be used with operands", {
                let mut snippets =
                    Snippet::with_primary_span(option_location, "`-a` specified here".into());
                add_span(
                    &operand_location.code,
                    Span {
                        range: operand_location.byte_range(),
                        role: SpanRole::Primary {
                            label: "operand specified here".into(),
                        },
                    },
                    &mut snippets,
                );
                snippets
            }),

            Self::MissingArgument => ("no option or operand specified", vec![]),
        };

        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = title.into();
        report.snippets = snippets;
        report
    }

    /// Converts the error into a [`Message`].
    #[allow(deprecated)]
    #[deprecated(note = "use `to_report` instead", since = "0.11.0")]
    pub fn to_message(&self) -> Message<'_> {
        let (title, annotations) = match self {
            Error::CommonError(e) => return e.into(),

            Error::ConflictingOptionAndOperand {
                option_location,
                operand_location,
            } => (
                "cannot specify `-a` with other operands".into(),
                vec![
                    Annotation::new(
                        AnnotationType::Error,
                        "cannot specify `-a` with other operands".into(),
                        option_location,
                    ),
                    Annotation::new(
                        AnnotationType::Error,
                        "operand specified here".into(),
                        operand_location,
                    ),
                ],
            ),

            Error::MissingArgument => ("no option or operand specified".into(), vec![]),
        };

        Message {
            r#type: AnnotationType::Error,
            title,
            annotations,
            footers: vec![],
        }
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

#[allow(deprecated)]
impl<'a> From<&'a Error> for Message<'a> {
    #[inline]
    fn from(e: &'a Error) -> Self {
        e.to_message()
    }
}

/// Parses command line arguments for the `unalias` built-in.
pub fn parse(env: &Env, args: Vec<Field>) -> Result<Command, Error> {
    let mode = Mode::with_env(env);
    let (mut options, operands) = parse_arguments(OPTION_SPECS, mode, args)?;

    for option in &options {
        debug_assert_eq!(option.spec.get_short(), Some('a'));
    }

    match (options.pop(), operands.is_empty()) {
        (None, true) => Err(Error::MissingArgument),
        (None, false) => Ok(Command::Remove(operands)),
        (Some(_), true) => Ok(Command::RemoveAll),
        (Some(option), false) => Err(Error::ConflictingOptionAndOperand {
            option_location: option.location,
            operand_location: { operands }.swap_remove(0).origin,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_option() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-a"]));
        assert_eq!(result, Ok(Command::RemoveAll));
    }

    #[test]
    fn operands() {
        let env = Env::new_virtual();
        let operands = Field::dummies(["foo", "bar"]);
        let result = parse(&env, operands.clone());
        assert_eq!(result, Ok(Command::Remove(operands)));
    }

    #[test]
    fn missing_arguments() {
        let env = Env::new_virtual();
        let result = parse(&env, vec![]);
        assert_eq!(result, Err(Error::MissingArgument));
    }

    #[test]
    fn option_and_operand() {
        let env = Env::new_virtual();
        let args = Field::dummies(["-a", "foo"]);
        let result = parse(&env, args);
        assert_eq!(
            result,
            Err(Error::ConflictingOptionAndOperand {
                option_location: Location::dummy("-a"),
                operand_location: Location::dummy("foo"),
            }),
        );
    }
}
