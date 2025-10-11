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

//! Command line argument parser for the command built-in

use super::Command;
use super::Identify;
use super::Invoke;
use super::Search;
use crate::common::syntax::Mode;
use crate::common::syntax::OptionOccurrence;
use crate::common::syntax::OptionSpec;
use crate::common::syntax::ParseError;
use crate::common::syntax::parse_arguments;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
#[allow(deprecated)]
use yash_syntax::source::pretty::Message;
use yash_syntax::source::pretty::Report;

/// Error in parsing command line arguments
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common parser.
    #[error(transparent)]
    CommonError(#[from] ParseError<'static>),
    // TODO MissingCommandName
    // TODO TooManyCommandNames
    // TODO UninvokableCategory
}

impl Error {
    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        match self {
            Self::CommonError(e) => e.to_report(),
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
    fn from(error: &'a Error) -> Self {
        match error {
            Error::CommonError(error) => error.into(),
        }
    }
}

const OPTION_SPECS: &[OptionSpec] = &[
    OptionSpec::new().short('p').long("path"),
    OptionSpec::new().short('v').long("identify"),
    OptionSpec::new().short('V').long("verbose-identify"),
];

/// Interprets the parsed command line arguments
///
/// This function converts the result of [`parse_arguments`] into a `Command`.
pub fn interpret(
    options: Vec<OptionOccurrence<'_>>,
    operands: Vec<Field>,
) -> Result<Command, Error> {
    // Interpret options
    let mut standard_path = false;
    let mut verbose_identify = None;
    for option in options {
        match option.spec.get_short() {
            Some('p') => standard_path = true,
            Some('v') => verbose_identify = Some(false),
            Some('V') => verbose_identify = Some(true),
            _ => unreachable!("unhandled option: {:?}", option),
        }
    }

    // Produce the result
    if let Some(verbose) = verbose_identify {
        let mut search = Search::default_for_identify();
        search.standard_path = standard_path;
        let identify = Identify {
            names: operands,
            search,
            verbose,
        };
        Ok(identify.into())
    } else {
        let mut search = Search::default_for_invoke();
        search.standard_path = standard_path;
        let fields = operands;
        let invoke = Invoke { fields, search };
        Ok(invoke.into())
    }
}

/// Parses command line arguments of the `command` built-in
pub fn parse(env: &Env, args: Vec<Field>) -> Result<Command, Error> {
    let (options, operands) = parse_arguments(OPTION_SPECS, Mode::with_env(env), args)?;
    interpret(options, operands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Category;
    use assert_matches::assert_matches;
    use enumset::EnumSet;

    #[test]
    fn invoke_without_options() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["foo", "bar", "baz"]));

        assert_matches!(result, Ok(Command::Invoke(invoke)) => {
            assert_eq!(invoke.fields, Field::dummies(["foo", "bar", "baz"]));
            assert_eq!(
                invoke.search,
                Search {
                    standard_path: false,
                    categories: Category::Builtin | Category::ExternalUtility
                }
            );
        });
    }

    #[test]
    fn invoke_with_p_option() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-p", "foo"]));

        assert_matches!(result, Ok(Command::Invoke(invoke)) => {
            assert_eq!(invoke.fields, Field::dummies(["foo"]));
            assert_eq!(
                invoke.search,
                Search {
                    standard_path: true,
                    categories: Category::Builtin | Category::ExternalUtility
                }
            );
        });
    }

    #[test]
    fn identify_without_options() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-v", "foo"]));

        assert_matches!(result, Ok(Command::Identify(identify)) => {
            assert_eq!(identify.names, Field::dummies(["foo"]));
            assert_eq!(
                identify.search,
                Search {
                    standard_path: false,
                    categories: EnumSet::all()
                }
            );
            assert!(!identify.verbose);
        });
    }

    #[test]
    fn identify_with_p_option() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-v", "-p", "foo"]));

        assert_matches!(result, Ok(Command::Identify(identify)) => {
            assert_eq!(identify.names, Field::dummies(["foo"]));
            assert_eq!(
                identify.search,
                Search {
                    standard_path: true,
                    categories: EnumSet::all()
                }
            );
            assert!(!identify.verbose);
        });
    }

    #[test]
    fn verbosely_identify_without_options() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-V", "bar"]));

        assert_matches!(result, Ok(Command::Identify(identify)) => {
            assert_eq!(identify.names, Field::dummies(["bar"]));
            assert_eq!(
                identify.search,
                Search {
                    standard_path: false,
                    categories: EnumSet::all()
                }
            );
            assert!(identify.verbose);
        });
    }

    // This ordering is not specified by POSIX, but it is consistent with the
    // older versions of yash.
    #[test]
    #[allow(non_snake_case)]
    fn last_specified_option_wins_between_v_and_V() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-V", "-v", "baz"]));
        assert_matches!(result, Ok(Command::Identify(identify)) => {
            assert!(!identify.verbose);
        });

        let result = parse(&env, Field::dummies(["-v", "-V", "baz"]));
        assert_matches!(result, Ok(Command::Identify(identify)) => {
            assert!(identify.verbose);
        });
    }
}
