// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! This crate implements arithmetic expansion.
//!
//! TODO Elaborate

use std::fmt::Display;
use std::ops::Range;

/// Result of arithmetic expansion
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Value {
    Integer(i64),
    // TODO Float, String
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Integer(i) => i.fmt(f),
        }
    }
}

/// Cause of an arithmetic expansion error
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ErrorCause {
    // TODO Error cause variants
}

impl Display for ErrorCause {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

/// Description of an error that occurred during expansion
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Error {
    /// Cause of the error
    cause: ErrorCause,
    /// Range of the substring in the evaluated expression string where the error occurred
    location: Range<usize>,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}

impl std::error::Error for Error {}

// TODO Variable environment
/// Performs arithmetic expansion
pub fn eval(_expression: &str) -> Result<Value, Error> {
    // TODO Implement arithmetic expansion
    Ok(Value::Integer(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = eval("0");
        assert_eq!(result, Ok(Value::Integer(0)));
    }
}
