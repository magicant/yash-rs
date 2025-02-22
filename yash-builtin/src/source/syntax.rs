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
use crate::common::report_error;
use crate::common::report_simple_error;
use crate::common::syntax::Mode;
use crate::common::syntax::ParseError;
use crate::common::syntax::parse_arguments;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;

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
}

impl Error {
    /// Reports the error to the standard error.
    pub async fn report(&self, env: &mut Env) -> crate::Result {
        match self {
            Error::CommonError(e) => report_error(env, e).await,
            Error::MissingFile => report_simple_error(env, "missing file operand").await,
        }
    }
}

/// Parses command line arguments to the source built-in.
pub fn parse(env: &Env, args: Vec<Field>) -> Result<Command, Error> {
    let mode = Mode::with_env(env);
    let (_options, mut operands) = parse_arguments(&[], mode, args)?;
    if operands.is_empty() {
        return Err(Error::MissingFile);
    }
    let file = operands.remove(0);
    let params = operands;
    Ok(Command { file, params })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_only() {
        let env = Env::new_virtual();
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
}
