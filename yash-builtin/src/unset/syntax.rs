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

//! Parses the unset built-in command arguments.

use crate::common::syntax::ConflictingOptionError;
use crate::common::syntax::OptionSpec;
use crate::common::syntax::parse_arguments;
use thiserror::Error;
use yash_env::Env;
use yash_env::option::Option::Portable;
use yash_env::option::State;
use yash_env::semantics::Field;
use yash_env::source::pretty::Footnote;
use yash_env::source::pretty::FootnoteType;
use yash_env::source::pretty::Report;
use yash_env::source::pretty::ReportType;

use super::Command;
use super::Mode;

/// Error in parsing command line arguments
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common parser.
    #[error(transparent)]
    CommonError(#[from] crate::common::syntax::ParseError<'static>),

    /// The `-f` and `-v` options are used together.
    #[error(transparent)]
    ConflictingOption(#[from] ConflictingOptionError<'static>),

    /// No operand is given while POSIX portability is required.
    #[error("missing operand")]
    MissingOperand,
}

impl Error {
    /// Converts the error to a report.
    pub fn to_report(&self) -> Report<'_> {
        match self {
            Error::CommonError(inner) => inner.to_report(),
            Error::ConflictingOption(inner) => inner.to_report(),

            Error::MissingOperand => {
                // There is no operand to annotate, so the report has no snippet of
                // its own. The built-in name is annotated by the caller.
                let mut report = Report::new();
                report.r#type = ReportType::Error;
                report.title = self.to_string().into();
                report.footnotes.push(Footnote {
                    r#type: FootnoteType::Note,
                    label: "this error is reported because the `portable` shell option is enabled"
                        .into(),
                });
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
pub type Result = std::result::Result<Command, Error>;

const OPTION_SPECS: &[OptionSpec] = &[
    OptionSpec::new().short('f').long("functions"),
    OptionSpec::new().short('v').long("variables"),
];

/// Parses command line arguments for the unset built-in.
///
/// While the [`Portable`] shell option is on, the built-in requires at least
/// one operand, as POSIX specifies the syntax as `unset [-fv] name…`. An
/// invocation without operands is rejected with [`Error::MissingOperand`].
pub fn parse<S>(env: &Env<S>, args: Vec<Field>) -> Result {
    let parser_mode = crate::common::syntax::Mode::with_env(env);
    let (options, operands) = parse_arguments(OPTION_SPECS, parser_mode, args)?;

    // Decide which to unset: variables or functions.
    let f_option = options.iter().position(|o| o.spec.get_short() == Some('f'));
    let v_option = options.iter().position(|o| o.spec.get_short() == Some('v'));
    let mode = match (f_option, v_option) {
        (None, None) => Mode::default(),
        (None, Some(_)) => Mode::Variables,
        (Some(_), None) => Mode::Functions,
        (Some(f_pos), Some(v_pos)) => {
            return Err(ConflictingOptionError::pick_from_indexes(options, [f_pos, v_pos]).into());
        }
    };

    let names = operands;
    if names.is_empty() && env.options.get(Portable) == State::On {
        return Err(Error::MissingOperand);
    }

    Ok(Command { mode, names })
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn no_arguments_without_portable() {
        let env = Env::new_virtual();
        let result = parse(&env, vec![]);
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::Variables,
                names: vec![],
            })
        );
    }

    #[test]
    fn no_arguments_portable() {
        // With the portable option on, the built-in requires an operand.
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        let result = parse(&env, vec![]);
        assert_eq!(result, Err(Error::MissingOperand));
    }

    #[test]
    fn no_operands_with_option_portable() {
        // The operand is required regardless of the -f and -v options.
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        let result = parse(&env, Field::dummies(["-v"]));
        assert_eq!(result, Err(Error::MissingOperand));

        let result = parse(&env, Field::dummies(["-f"]));
        assert_eq!(result, Err(Error::MissingOperand));
    }

    #[test]
    fn operands_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        let args = Field::dummies(["foo"]);
        let result = parse(&env, args.clone());
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::Variables,
                names: args,
            })
        );
    }

    #[test]
    fn conflicting_options_take_precedence_over_missing_operand() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        let result = parse(&env, Field::dummies(["-f", "-v"]));
        assert_matches!(result, Err(Error::ConflictingOption(_)));
    }

    #[test]
    fn missing_operand_report_mentions_portable_option() {
        let report = Error::MissingOperand.to_report();
        assert_matches!(&report.footnotes[..], [footnote] => {
            assert_eq!(footnote.r#type, FootnoteType::Note);
            assert!(
                footnote.label.contains("portable"),
                "footnote label should contain `portable`: {:?}",
                footnote.label,
            );
        });
    }

    #[test]
    fn v_option() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-v"]));
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::Variables,
                names: vec![],
            })
        );

        // The same option can be specified multiple times.
        let result = parse(&env, Field::dummies(["-vv", "--variables"]));
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::Variables,
                names: vec![],
            })
        );
    }

    #[test]
    fn f_option() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-f"]));
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::Functions,
                names: vec![],
            })
        );

        // The same option can be specified multiple times.
        let result = parse(&env, Field::dummies(["-ff", "--functions"]));
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::Functions,
                names: vec![],
            })
        );
    }

    #[test]
    fn v_and_f_option() {
        // Specifying both -v and -f is an error.
        let env = Env::new_virtual();
        let args = Field::dummies(["-fv"]);
        let result = parse(&env, args.clone());
        assert_matches!(result, Err(Error::ConflictingOption(error)) => {
            let short_options = error
                .options()
                .iter()
                .map(|o| o.spec.get_short())
                .collect::<Vec<_>>();
            assert_eq!(short_options, [Some('f'), Some('v')], "{error:?}");
        });
    }

    #[test]
    fn operands() {
        let env = Env::new_virtual();
        let args = Field::dummies(["foo", "bar"]);
        let result = parse(&env, args.clone());
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::Variables,
                names: args,
            })
        );
    }
}
