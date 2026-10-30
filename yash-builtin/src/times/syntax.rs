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

//! Command line syntax parsing for the times built-in

use crate::common::help::{HELP_OPTION, OptionSpecsWithHelp, WithHelp, parse_arguments_with_help};
use crate::common::syntax::Mode;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::source::pretty::{Report, ReportType, Snippet};

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
    /// Converts the error to a report.
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
                    format!("{}: unexpected operand", operands[0]).into(),
                );
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

pub(super) const OPTION_SPECS: OptionSpecsWithHelp = OptionSpecsWithHelp::new(&[HELP_OPTION]);

/// Parses command line arguments for the times built-in.
pub fn parse<S>(env: &Env<S>, args: Vec<Field>) -> Result<WithHelp<()>, Error> {
    let WithHelp::Main((options, operands)) =
        parse_arguments_with_help(OPTION_SPECS, Mode::with_env(env), args)?
    else {
        return Ok(WithHelp::Help);
    };
    debug_assert_eq!(options, []);

    if operands.is_empty() {
        Ok(WithHelp::Main(()))
    } else {
        Err(Error::UnexpectedOperands(operands))
    }
}
