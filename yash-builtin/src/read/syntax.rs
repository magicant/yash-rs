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

//! Command line argument parser for the read built-in

use super::Command;
use crate::common::syntax::Mode;
use crate::common::syntax::OptionArgumentSpec;
use crate::common::syntax::OptionSpec;
use crate::common::syntax::parse_arguments;
use thiserror::Error;
use yash_env::Env;
use yash_env::option::Option::Portable;
use yash_env::option::State;
use yash_env::semantics::Field;
use yash_env::source::pretty::Snippet;
use yash_env::source::pretty::{Footnote, FootnoteType, Report, ReportType};
use yash_env::variable::is_portable_variable_name;

/// Error in parsing command line arguments
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common parser.
    #[error(transparent)]
    CommonError(#[from] crate::common::syntax::ParseError<'static>),

    /// The delimiter specified by the `-d` option is multibyte.
    #[error("multibyte delimiter is not supported")]
    MultibyteDelimiter { delimiter: Field },

    /// No operand is given.
    #[error("missing operand")]
    MissingOperand,

    /// An operand is not a valid variable name.
    #[error("invalid variable name")]
    InvalidVariableName { name: Field },

    /// An operand is not a portable variable name while the `portable` shell
    /// option is on.
    #[error("non-portable variable name")]
    NonPortableVariableName { name: Field },
}

impl Error {
    /// Converts this error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let snippets = match self {
            Self::CommonError(parse_error) => return parse_error.to_report(),

            Self::MultibyteDelimiter { delimiter } => Snippet::with_primary_span(
                &delimiter.origin,
                format!(
                    "delimiter {:?} is {}-byte long",
                    delimiter.value,
                    delimiter.value.len()
                )
                .into(),
            ),

            Self::MissingOperand => vec![],

            Self::InvalidVariableName { name } => Snippet::with_primary_span(
                &name.origin,
                format!("variable name {:?} is not valid", name.value).into(),
            ),

            Self::NonPortableVariableName { name } => Snippet::with_primary_span(
                &name.origin,
                format!("variable name {:?} is not portable", name.value).into(),
            ),
        };

        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();
        report.snippets = snippets;
        if let Self::NonPortableVariableName { .. } = self {
            report.footnotes.push(Footnote {
                r#type: FootnoteType::Note,
                label: "this error is reported because the `portable` shell option is enabled"
                    .into(),
            });
        }
        report
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

const OPTION_SPECS: &[OptionSpec] = &[
    OptionSpec::new()
        .short('d')
        .long("delimiter")
        .argument(OptionArgumentSpec::Required),
    OptionSpec::new().short('r').long("raw-mode"),
];

/// Parses command line arguments.
pub fn parse<S>(env: &Env<S>, args: Vec<Field>) -> Result<Command, Error> {
    let mode = Mode::with_env(env);
    let (options, operands) = parse_arguments(OPTION_SPECS, mode, args)?;

    // Parse options
    let mut delimiter = b'\n';
    let mut is_raw = false;
    for option in options {
        match option.spec.get_short() {
            Some('d') => {
                let arg = option.argument.unwrap();
                match arg.value.len() {
                    0 => delimiter = b'\0',
                    1 => delimiter = arg.value.as_bytes()[0],
                    _ => return Err(Error::MultibyteDelimiter { delimiter: arg }),
                }
            }
            Some('r') => is_raw = true,
            _ => unreachable!(),
        }
    }

    // Parse operands
    let portable = env.options.get(Portable) == State::On;
    let mut variables = validate_names(operands, portable)?;
    let last_variable = variables.pop().ok_or(Error::MissingOperand)?;

    Ok(Command {
        delimiter,
        is_raw,
        variables,
        last_variable,
    })
}

/// Tests if all the variable names are valid.
///
/// If all the variable names are valid, this function returns `names` as is.
/// Otherwise, this function returns an `Error::InvalidVariableName`.
/// If `portable` is `true`, names that are not portable variable names are
/// also rejected with an `Error::NonPortableVariableName`.
fn validate_names(names: Vec<Field>, portable: bool) -> Result<Vec<Field>, Error> {
    if let Some(i) = names.iter().position(|name| name.value.contains('=')) {
        return Err(Error::InvalidVariableName {
            name: { names }.swap_remove(i),
        });
    }
    if portable
        && let Some(i) = names
            .iter()
            .position(|name| !is_portable_variable_name(&name.value))
    {
        return Err(Error::NonPortableVariableName {
            name: { names }.swap_remove(i),
        });
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_operand() {
        let env = Env::new_virtual();
        assert_eq!(
            parse(&env, Field::dummies(["var"])),
            Ok(Command {
                delimiter: b'\n',
                is_raw: false,
                variables: vec![],
                last_variable: Field::dummy("var"),
            })
        );
    }

    #[test]
    fn raw_mode() {
        let env = Env::new_virtual();
        assert_eq!(
            parse(&env, Field::dummies(["-r", "var"])),
            Ok(Command {
                delimiter: b'\n',
                is_raw: true,
                variables: vec![],
                last_variable: Field::dummy("var"),
            })
        );
    }

    #[test]
    fn nul_delimiter() {
        let env = Env::new_virtual();
        assert_eq!(
            parse(&env, Field::dummies(["-d", "", "var"])),
            Ok(Command {
                delimiter: b'\0',
                is_raw: false,
                variables: vec![],
                last_variable: Field::dummy("var"),
            })
        );
    }

    #[test]
    fn non_default_non_nul_delimiter() {
        let env = Env::new_virtual();
        assert_eq!(
            parse(&env, Field::dummies(["-d", ":", "var"])),
            Ok(Command {
                delimiter: b':',
                is_raw: false,
                variables: vec![],
                last_variable: Field::dummy("var"),
            })
        );
    }

    #[test]
    fn multibyte_delimiter_is_not_supported() {
        let env = Env::new_virtual();
        assert_eq!(
            parse(&env, Field::dummies(["-d", "!?", "var"])),
            Err(Error::MultibyteDelimiter {
                delimiter: Field::dummy("!?")
            })
        );

        assert_eq!(
            parse(&env, Field::dummies(["-d", "あ", "var"])),
            Err(Error::MultibyteDelimiter {
                delimiter: Field::dummy("あ")
            })
        );
    }

    #[test]
    fn many_operands() {
        let env = Env::new_virtual();
        assert_eq!(
            parse(&env, Field::dummies(["foo", "bar"])),
            Ok(Command {
                delimiter: b'\n',
                is_raw: false,
                variables: Field::dummies(["foo"]),
                last_variable: Field::dummy("bar"),
            })
        );

        assert_eq!(
            parse(&env, Field::dummies(["first", "second", "third"])),
            Ok(Command {
                delimiter: b'\n',
                is_raw: false,
                variables: Field::dummies(["first", "second"]),
                last_variable: Field::dummy("third"),
            })
        );
    }

    #[test]
    fn missing_operand() {
        let env = Env::new_virtual();
        assert_eq!(parse(&env, vec![]), Err(Error::MissingOperand));
    }

    #[test]
    fn operand_containing_equal() {
        let env = Env::new_virtual();
        assert_eq!(
            parse(&env, Field::dummies(["="])),
            Err(Error::InvalidVariableName {
                name: Field::dummy("=")
            })
        );
        assert_eq!(
            parse(&env, Field::dummies(["foo", "bar=bar", "baz"])),
            Err(Error::InvalidVariableName {
                name: Field::dummy("bar=bar")
            })
        );
    }

    #[test]
    fn non_portable_variable_name_with_portable_option() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        assert_eq!(
            parse(&env, Field::dummies(["foo-bar"])),
            Err(Error::NonPortableVariableName {
                name: Field::dummy("foo-bar")
            })
        );
        assert_eq!(
            parse(&env, Field::dummies(["ok", "1abc", "baz"])),
            Err(Error::NonPortableVariableName {
                name: Field::dummy("1abc")
            })
        );
        assert_eq!(
            parse(&env, Field::dummies([""])),
            Err(Error::NonPortableVariableName {
                name: Field::dummy("")
            })
        );
    }

    #[test]
    fn operand_containing_equal_with_portable_option() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        assert_eq!(
            parse(&env, Field::dummies(["bar=bar"])),
            Err(Error::InvalidVariableName {
                name: Field::dummy("bar=bar")
            })
        );
    }

    #[test]
    fn portable_variable_name_with_portable_option() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        assert_eq!(
            parse(&env, Field::dummies(["foo", "_bar123"])),
            Ok(Command {
                delimiter: b'\n',
                is_raw: false,
                variables: Field::dummies(["foo"]),
                last_variable: Field::dummy("_bar123"),
            })
        );
    }

    #[test]
    fn non_portable_variable_name_without_portable_option() {
        let env = Env::new_virtual();
        assert_eq!(
            parse(&env, Field::dummies(["foo-bar"])),
            Ok(Command {
                delimiter: b'\n',
                is_raw: false,
                variables: vec![],
                last_variable: Field::dummy("foo-bar"),
            })
        );
    }
}
