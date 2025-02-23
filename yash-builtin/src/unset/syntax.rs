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
use std::borrow::Cow;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::MessageBase;

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
    // TODO MissingOperand
}

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        self.to_string().into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        match self {
            Error::CommonError(inner) => inner.main_annotation(),
            Error::ConflictingOption(inner) => inner.main_annotation(),
        }
    }

    fn additional_annotations<'a, T: Extend<Annotation<'a>>>(&'a self, results: &mut T) {
        match self {
            Error::CommonError(inner) => inner.additional_annotations(results),
            Error::ConflictingOption(inner) => inner.additional_annotations(results),
        }
    }
}

/// Result of parsing command line arguments
pub type Result = std::result::Result<Command, Error>;

const OPTION_SPECS: &[OptionSpec] = &[
    OptionSpec::new().short('f').long("functions"),
    OptionSpec::new().short('v').long("variables"),
];

/// Parses command line arguments for the unset built-in.
pub fn parse(env: &Env, args: Vec<Field>) -> Result {
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
    Ok(Command { mode, names })
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn no_arguments_non_posix() {
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

    // TODO no_arguments_posix: In the POSIXly-correct mode, the built-in
    // requires at least one operand.

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
