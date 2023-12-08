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
use crate::common::syntax::parse_arguments;
use crate::common::syntax::Mode;
use crate::common::syntax::OptionSpec;
use thiserror::Error;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;

/// Error in parsing command line arguments
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common parser.
    #[error(transparent)]
    CommonError(#[from] crate::common::syntax::ParseError<'static>),

    /// No operand is given.
    #[error("missing operand")]
    MissingOperand,
}

impl Error {
    /// Converts this error into a message.
    pub fn to_message(&self) -> Message {
        match self {
            Error::CommonError(e) => e.into(),

            Error::MissingOperand => Message {
                r#type: AnnotationType::Error,
                title: self.to_string().into(),
                annotations: vec![],
            },
        }
    }
}

impl<'a> From<&'a Error> for Message<'a> {
    #[inline]
    fn from(e: &'a Error) -> Self {
        e.to_message()
    }
}

const OPTION_SPECS: &[OptionSpec] = &[OptionSpec::new().short('r').long("raw-mode")];

/// Parses command line arguments.
pub fn parse(env: &Env, args: Vec<Field>) -> Result<Command, Error> {
    let mode = Mode::with_env(env);
    let (options, operands) = parse_arguments(OPTION_SPECS, mode, args)?;

    // Parse options
    let mut is_raw = false;
    for option in options {
        match option.spec.get_short() {
            Some('r') => is_raw = true,
            _ => unreachable!(),
        }
    }

    // Parse operands
    let mut variables = operands;
    let last_variable = variables.pop().ok_or(Error::MissingOperand)?;

    Ok(Command {
        is_raw,
        variables,
        last_variable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_raw_mode() {
        let env = Env::new_virtual();
        assert_eq!(
            parse(&env, Field::dummies(["var"])),
            Ok(Command {
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
                is_raw: true,
                variables: vec![],
                last_variable: Field::dummy("var"),
            })
        );
    }

    #[test]
    fn many_operands() {
        let env = Env::new_virtual();
        assert_eq!(
            parse(&env, Field::dummies(["foo", "bar"])),
            Ok(Command {
                is_raw: false,
                variables: Field::dummies(["foo"]),
                last_variable: Field::dummy("bar"),
            })
        );

        assert_eq!(
            parse(&env, Field::dummies(["first", "second", "third"])),
            Ok(Command {
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
}
