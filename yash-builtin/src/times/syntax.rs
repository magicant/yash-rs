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

use crate::common::syntax::{parse_arguments, Mode};
use std::borrow::Cow;
use thiserror::Error;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_syntax::source::pretty::{Annotation, AnnotationType, MessageBase};

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

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        self.to_string().into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        use Error::*;
        match self {
            CommonError(e) => e.main_annotation(),
            UnexpectedOperands(operands) => Annotation::new(
                AnnotationType::Error,
                format!("{}: unexpected operand", operands[0].value).into(),
                &operands[0].origin,
            ),
        }
    }
}

/// Parses command line arguments for the times built-in.
pub fn parse(env: &Env, args: Vec<Field>) -> Result<(), Error> {
    let (options, operands) = parse_arguments(&[], Mode::with_env(env), args)?;
    debug_assert_eq!(options, []);

    if operands.is_empty() {
        Ok(())
    } else {
        Err(Error::UnexpectedOperands(operands))
    }
}
