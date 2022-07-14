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

/// Intermediate result of evaluating part of an expression
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Term {
    /// Value
    Value(Value),
    // TODO Variable
}

/// Cause of an arithmetic expansion error
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ErrorCause {
    /// A value expression contains an invalid character.
    InvalidCharacterInValue,
}

impl Display for ErrorCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCause::InvalidCharacterInValue => "invalid character in value".fmt(f),
        }
    }
}

/// Description of an error that occurred during expansion
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Error {
    /// Cause of the error
    pub cause: ErrorCause,
    /// Range of the substring in the evaluated expression string where the error occurred
    pub location: Range<usize>,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}

impl std::error::Error for Error {}

mod token;

use token::Token;
use token::Tokens;

// TODO Variable environment
/// Performs arithmetic expansion
pub fn eval(expression: &str) -> Result<Value, Error> {
    let mut tokens = Tokens::new(expression);
    match tokens.next() {
        Some(Ok(Token::Term(Term::Value(value)))) => Ok(value),
        Some(Err(error)) => Err(error),
        other => todo!("handle token {:?}", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_integer_constants() {
        assert_eq!(eval("1"), Ok(Value::Integer(1)));
        assert_eq!(eval("42"), Ok(Value::Integer(42)));
    }

    #[test]
    fn octal_integer_constants() {
        assert_eq!(eval("0"), Ok(Value::Integer(0)));
        assert_eq!(eval("01"), Ok(Value::Integer(1)));
        assert_eq!(eval("07"), Ok(Value::Integer(7)));
        assert_eq!(eval("0123"), Ok(Value::Integer(0o123)));
    }

    #[test]
    fn invalid_digit_in_octal_constant() {
        assert_eq!(
            eval("08"),
            Err(Error {
                cause: ErrorCause::InvalidCharacterInValue,
                location: 0..2,
            })
        );
        assert_eq!(
            eval("0192"),
            Err(Error {
                cause: ErrorCause::InvalidCharacterInValue,
                location: 0..4,
            })
        );
    }

    #[test]
    fn space_around_token() {
        assert_eq!(eval(" 12"), Ok(Value::Integer(12)));
        assert_eq!(eval("12 "), Ok(Value::Integer(12)));
        assert_eq!(eval("\n 123 \t"), Ok(Value::Integer(123)));
        // TODO Test with more complex expressions
    }

    // TODO Variables (integers, floats, infinities, & NaNs)
    // TODO Operators
}
