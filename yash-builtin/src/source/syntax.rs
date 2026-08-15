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

//! Parsing command line arguments to the source built-in

use super::Command;
use crate::common::report::report_error;
use crate::common::syntax::Mode;
use crate::common::syntax::ParseError;
use crate::common::syntax::parse_arguments;
use thiserror::Error;
use yash_env::Env;
use yash_env::option::Option::Portable;
use yash_env::option::State;
use yash_env::semantics::Field;
use yash_env::source::pretty::Footnote;
use yash_env::source::pretty::FootnoteType;
use yash_env::source::pretty::Snippet;
use yash_env::source::pretty::{Report, ReportType};
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;

/// Error in parsing command line arguments
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common parser.
    #[error(transparent)]
    CommonError(#[from] ParseError<'static>),

    /// The file to be executed is not specified.
    #[error("missing file operand")]
    MissingFile,

    /// An operand is given after the file operand while POSIX portability is
    /// required.
    ///
    /// Operands after the file are a non-portable extension: POSIX specifies
    /// the syntax as `. file`. The contained vector holds all such operands and
    /// is never empty.
    #[error("non-portable operand {:?}", .0[0].value)]
    NonPortableOperand(Vec<Field>),
}

impl Error {
    /// Converts this error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        match self {
            Self::CommonError(e) => e.to_report(),

            Self::MissingFile => {
                let mut report = Report::new();
                report.r#type = ReportType::Error;
                report.title = "missing file operand".into();
                report
            }

            Self::NonPortableOperand(operands) => {
                let mut report = Report::new();
                report.r#type = ReportType::Error;
                report.title = self.to_string().into();
                // Annotating every operand would only repeat the same label, so
                // the report marks the first one and lets the title speak for
                // the rest.
                report.snippets = Snippet::with_primary_span(
                    &operands[0].origin,
                    "POSIX does not allow an operand after the file operand".into(),
                );
                report.footnotes.push(Footnote {
                    r#type: FootnoteType::Note,
                    label: "this error is reported because the `portable` shell option is enabled"
                        .into(),
                });
                report
            }
        }
    }

    /// Reports the error to the standard error.
    #[inline(always)]
    pub async fn report<S>(&self, env: &mut Env<S>) -> crate::Result
    where
        S: Isatty + WriteAll,
    {
        report_error(env, self).await
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

/// Parses command line arguments to the source built-in.
///
/// While the [`Portable`] shell option is on, an operand after the file is
/// rejected with [`Error::NonPortableOperand`], since POSIX specifies the
/// syntax as `. file`.
pub fn parse<S>(env: &Env<S>, args: Vec<Field>) -> Result<Command, Error> {
    let mode = Mode::with_env(env);
    let (_options, mut operands) = parse_arguments(&[], mode, args)?;
    if operands.is_empty() {
        return Err(Error::MissingFile);
    }
    let file = operands.remove(0);
    let params = operands;

    if !params.is_empty() && env.options.get(Portable) == State::On {
        return Err(Error::NonPortableOperand(params));
    }

    Ok(Command { file, params })
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn file_and_parameters() {
        let env = Env::new_virtual();
        let args = Field::dummies(["my/file", "foo", "bar"]);
        assert_eq!(
            parse(&env, args),
            Ok(Command {
                file: Field::dummy("my/file"),
                params: Field::dummies(["foo", "bar"]),
            })
        );
    }

    #[test]
    fn specifying_file_only_is_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        let args = vec![Field::dummy("foo")];
        assert_eq!(
            parse(&env, args),
            Ok(Command {
                file: Field::dummy("foo"),
                params: vec![],
            })
        );
    }

    #[test]
    fn specifying_file_and_parameters_is_non_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        let args = Field::dummies(["my/file", "foo", "bar"]);
        assert_eq!(
            parse(&env, args),
            Err(Error::NonPortableOperand(Field::dummies(["foo", "bar"]))),
        );
    }

    #[test]
    fn no_file() {
        let env = Env::new_virtual();
        let args = vec![];
        assert_eq!(parse(&env, args), Err(Error::MissingFile));
    }

    #[test]
    fn unknown_short_option() {
        let env = Env::new_virtual();
        let args = Field::dummies(["-@", "foo"]);
        assert_eq!(
            parse(&env, args),
            Err(Error::CommonError(ParseError::UnknownShortOption(
                '@',
                Field::dummy("-@"),
            ))),
        );
    }

    #[test]
    fn non_portable_operand_report_annotates_first_operand_only() {
        let error = Error::NonPortableOperand(Field::dummies(["foo", "bar"]));
        let report = error.to_report();
        assert_eq!(report.title, "non-portable operand \"foo\"");
        assert_matches!(&report.snippets[..], [snippet] => {
            assert_eq!(snippet.code_string(), "foo");
        });
        assert_matches!(&report.footnotes[..], [portable] => {
            assert_eq!(
                portable.label,
                "this error is reported because the `portable` shell option is enabled",
            );
        });
    }
}
