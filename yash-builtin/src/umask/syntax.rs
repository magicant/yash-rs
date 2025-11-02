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

//! Parsing command line arguments to the `umask` built-in

use super::Command;
use super::symbol::{ParseClausesError, parse_clauses};
use crate::common::syntax::{Mode, OptionSpec, ParseError, parse_arguments};
use std::borrow::Cow;
use std::num::ParseIntError;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
#[allow(deprecated)]
use yash_env::source::pretty::{Annotation, AnnotationType, MessageBase};
use yash_env::source::pretty::{Report, ReportType, Snippet};

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common syntax parser.
    #[error(transparent)]
    CommonError(#[from] ParseError<'static>),

    /// More than one operand is given.
    ///
    /// The vector contains *all* the operands, including the first proper one.
    #[error("too many operands")]
    TooManyOperands(Vec<Field>),

    /// An operand starts with a digit and is not a valid mode.
    #[error("invalid mask notation")]
    InvalidNumericMode(Field, ParseIntError),

    /// An operand does not start with a digit and is not a valid mode.
    #[error("invalid mask notation")]
    InvalidSymbolicMode(Field, ParseClausesError),
}

impl Error {
    /// Converts the error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let snippets = match self {
            Self::CommonError(e) => return e.to_report(),
            Self::TooManyOperands(operands) => Snippet::with_primary_span(
                &operands[1].origin,
                format!("{}: unexpected operand", operands[1].value).into(),
            ),
            Self::InvalidNumericMode(operand, parse_int_error) => Snippet::with_primary_span(
                &operand.origin,
                format!("{operand}: {parse_int_error}").into(),
            ),
            Self::InvalidSymbolicMode(operand, parse_clauses_error) => Snippet::with_primary_span(
                &operand.origin,
                format!("{operand}: {parse_clauses_error}").into(),
            ),
        };
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();
        report.snippets = snippets;
        report
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

#[allow(deprecated)]
impl MessageBase for Error {
    fn message_title(&self) -> Cow<'_, str> {
        self.to_string().into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        match self {
            Self::CommonError(e) => e.main_annotation(),
            Self::TooManyOperands(operands) => Annotation::new(
                AnnotationType::Error,
                format!("{}: unexpected operand", operands[1].value).into(),
                &operands[1].origin,
            ),
            Self::InvalidNumericMode(operand, e) => Annotation::new(
                AnnotationType::Error,
                format!("{}: {}", operand.value, e).into(),
                &operand.origin,
            ),
            Self::InvalidSymbolicMode(operand, e) => Annotation::new(
                AnnotationType::Error,
                format!("{}: {}", operand.value, e).into(),
                &operand.origin,
            ),
        }
    }
}

/// Result of parsing command line arguments
pub type Result = std::result::Result<Command, Error>;

/// List of all options supported by the `umask` built-in
const OPTION_SPECS: &[OptionSpec] = &[OptionSpec::new().short('S')];

/// Parses command line arguments.
pub fn parse(env: &Env, args: Vec<Field>) -> Result {
    let (options, operands) = parse_arguments(OPTION_SPECS, Mode::with_env(env), args)?;

    match operands.len() {
        0 => {
            let symbolic = options.iter().any(|o| o.spec.get_short() == Some('S'));
            Ok(Command::Show { symbolic })
        }

        1 => {
            let field = { operands }.pop().unwrap();

            // TODO Use char::is_ascii_octdigit
            if field.value.starts_with(|c: char| c.is_ascii_digit()) {
                return match u16::from_str_radix(&field.value, 8) {
                    Ok(mask) => Ok(Command::set_from_raw_mask(mask)),
                    Err(e) => Err(Error::InvalidNumericMode(field, e)),
                };
            }

            match parse_clauses(&field.value) {
                Ok(clauses) => Ok(Command::Set(clauses)),
                Err(e) => Err(Error::InvalidSymbolicMode(field, e)),
            }
        }

        _ => Err(Error::TooManyOperands(operands)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::umask::symbol::{Action, Clause, Operator, Permission, Who};
    use assert_matches::assert_matches;

    #[test]
    fn no_arguments() {
        let env = Env::new_virtual();
        let result = parse(&env, vec![]);
        assert_eq!(result, Ok(Command::Show { symbolic: false }));
    }

    #[test]
    fn symbolic_option() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-S"]));
        assert_eq!(result, Ok(Command::Show { symbolic: true }));
    }

    #[test]
    fn numeric_mask() {
        let env = Env::new_virtual();
        let args = Field::dummies(["022"]);
        let result = parse(&env, args);
        assert_eq!(
            result,
            Ok(Command::Set(vec![Clause {
                who: Who { mask: 0o777 },
                actions: vec![Action {
                    operator: Operator::Set,
                    permission: Permission::Literal {
                        mask: !0o022,
                        conditional_executable: false
                    },
                }],
            }]))
        );
    }

    #[test]
    fn symbolic_mask() {
        let env = Env::new_virtual();
        let args = Field::dummies(["u=rwx,go+r-w"]);
        let result = parse(&env, args);
        assert_eq!(
            result,
            Ok(Command::Set(vec![
                Clause {
                    who: Who { mask: 0o700 },
                    actions: vec![Action {
                        operator: Operator::Set,
                        permission: Permission::Literal {
                            mask: 0o777,
                            conditional_executable: false
                        }
                    }]
                },
                Clause {
                    who: Who { mask: 0o077 },
                    actions: vec![
                        Action {
                            operator: Operator::Add,
                            permission: Permission::Literal {
                                mask: 0o444,
                                conditional_executable: false
                            }
                        },
                        Action {
                            operator: Operator::Remove,
                            permission: Permission::Literal {
                                mask: 0o222,
                                conditional_executable: false
                            }
                        }
                    ]
                }
            ]))
        );
    }

    #[test]
    fn too_many_operands() {
        let env = Env::new_virtual();
        let args = Field::dummies(["022", "002"]);
        let result = parse(&env, args.clone());
        assert_eq!(result, Err(Error::TooManyOperands(args)));
    }

    #[test]
    fn operand_overrides_option() {
        // Currently, the `-S` option is ignored if the mode is given.
        let env = Env::new_virtual();
        let args = Field::dummies(["-S", "go=u"]);
        let result = parse(&env, args);
        assert_eq!(
            result,
            Ok(Command::Set(vec![Clause {
                who: Who { mask: 0o077 },
                actions: vec![Action {
                    operator: Operator::Set,
                    permission: Permission::CopyUser,
                }],
            }]))
        );
    }

    #[test]
    fn invalid_numeric_mask() {
        let env = Env::new_virtual();
        let arg = Field::dummy("02x2");
        let result = parse(&env, vec![arg.clone()]);
        assert_matches!(result, Err(Error::InvalidNumericMode(field, e)) => {
            assert_eq!(field, arg);
            assert_eq!(e.kind(), &std::num::IntErrorKind::InvalidDigit);
        });
    }

    #[test]
    fn numeric_mask_starting_with_plus() {
        let env = Env::new_virtual();
        let arg = Field::dummy("+022");
        let result = parse(&env, vec![arg.clone()]);
        assert_eq!(
            result,
            Err(Error::InvalidSymbolicMode(
                arg,
                ParseClausesError::InvalidChar('0')
            ))
        );
    }

    #[test]
    fn invalid_option() {
        // Though "-x" may look like a valid symbolic mode,
        // it is regarded as an invalid option without the "--" separator.
        let env = Env::new_virtual();
        let arg = Field::dummy("-x");
        let result = parse(&env, vec![arg.clone()]);
        assert_eq!(
            result,
            Err(Error::CommonError(ParseError::UnknownShortOption('x', arg)))
        );
    }
}
