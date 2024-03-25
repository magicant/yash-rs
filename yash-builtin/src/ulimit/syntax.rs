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

//! Command-line argument parser for the `ulimit` built-in

use super::Command;
use crate::common::syntax::ParseError;
use std::borrow::Cow;
use std::num::ParseIntError;
use thiserror::Error;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_syntax::source::pretty::{Annotation, MessageBase};

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common syntax parser.
    #[error(transparent)]
    CommonError(#[from] ParseError<'static>),

    /// More than one operand is given.
    ///
    /// The vector contains *all* the operands, including the first proper one.
    #[error("too many operands")]
    TooManyOperands(Vec<Field>),

    /// An operand is not a valid limit.
    #[error("invalid limit")]
    InvalidLimit(Field, ParseIntError),
}

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        todo!()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        todo!()
    }
}

/// Result of parsing command line arguments
pub type Result = std::result::Result<Command, Error>;

/// Parses command line arguments.
pub fn parse(env: &Env, args: Vec<Field>) -> Result {
    todo!()
}
