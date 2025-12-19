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

//! Command line argument parser for the pwd built-in

use super::Mode;
use crate::common::syntax::OptionOccurrence;
use crate::common::syntax::OptionSpec;
use crate::common::syntax::parse_arguments;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::source::pretty::Report;
use yash_env::source::pretty::ReportType;
use yash_env::source::pretty::Snippet;
use yash_env::source::pretty::Span;
use yash_env::source::pretty::SpanRole;
use yash_env::source::pretty::add_span;

/// Error in parsing command line arguments
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common parser.
    #[error(transparent)]
    CommonError(#[from] crate::common::syntax::ParseError<'static>),

    /// One or more operands are given.
    #[error("unexpected operand")]
    UnexpectedOperands(Vec<Field>),
}

impl Error {
    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        match self {
            Self::CommonError(e) => e.to_report(),

            Self::UnexpectedOperands(operands) => {
                let mut report = Report::new();
                report.r#type = ReportType::Error;
                report.title = "unexpected operand".into();
                report.snippets = Snippet::with_primary_span(
                    &operands[0].origin,
                    format!("{}: unexpected", operands[0]).into(),
                );
                for operand in &operands[1..] {
                    add_span(
                        &operand.origin.code,
                        Span {
                            range: operand.origin.byte_range(),
                            role: SpanRole::Primary {
                                label: format!("{}: unexpected", operand).into(),
                            },
                        },
                        &mut report.snippets,
                    );
                }
                report
            }
        }
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

/// Result of parsing command line arguments
pub type Result = std::result::Result<Mode, Error>;

const OPTION_SPECS: &[OptionSpec] = &[
    OptionSpec::new().short('L').long("logical"),
    OptionSpec::new().short('P').long("physical"),
];

fn mode_for_option(option: &OptionOccurrence) -> Mode {
    match option.spec.get_short() {
        Some('L') => Mode::Logical,
        Some('P') => Mode::Physical,
        _ => unreachable!(),
    }
}

/// Parses command line arguments for the pwd built-in.
pub fn parse<S>(env: &Env<S>, args: Vec<Field>) -> Result {
    let parser_mode = crate::common::syntax::Mode::with_env(env);
    let (options, operands) = parse_arguments(OPTION_SPECS, parser_mode, args)?;

    if !operands.is_empty() {
        return Err(Error::UnexpectedOperands(operands));
    }

    Ok(options.last().map(mode_for_option).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments() {
        let env = Env::new_virtual();
        let result = parse(&env, vec![]);
        assert_eq!(result, Ok(Mode::Logical));
    }

    #[test]
    fn logical_option() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-L"]));
        assert_eq!(result, Ok(Mode::Logical));
    }

    #[test]
    fn physical_option() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-P"]));
        assert_eq!(result, Ok(Mode::Physical));
    }

    #[test]
    fn last_option_wins() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-L", "-P"]));
        assert_eq!(result, Ok(Mode::Physical));

        let result = parse(&env, Field::dummies(["-P", "-L"]));
        assert_eq!(result, Ok(Mode::Logical));

        let result = parse(&env, Field::dummies(["-LPL"]));
        assert_eq!(result, Ok(Mode::Logical));

        let result = parse(&env, Field::dummies(["-PLP"]));
        assert_eq!(result, Ok(Mode::Physical));
    }

    #[test]
    fn unexpected_operand() {
        let env = Env::new_virtual();
        let args = Field::dummies(["foo"]);
        let result = parse(&env, args.clone());
        assert_eq!(result, Err(Error::UnexpectedOperands(args)));
    }

    #[test]
    fn unexpected_operands_after_options() {
        let env = Env::new_virtual();
        let args = Field::dummies(["-LP", "-L", "--", "one", "two"]);
        let operands = args[3..].to_vec();
        let result = parse(&env, args);
        assert_eq!(result, Err(Error::UnexpectedOperands(operands)));
    }
}
