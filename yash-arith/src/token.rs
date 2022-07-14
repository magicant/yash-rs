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

//! Tokenization

use super::Error;
use super::ErrorCause;
use super::Term;
use super::Value;

/// Atomic lexical element of an expression
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Token {
    /// Term
    Term(Term),
    // TODO Operators
}

/// Iterator extracting tokens from a string
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Tokens<'a> {
    source: &'a str,
    index: usize,
}

impl<'a> Tokens<'a> {
    /// Creates a tokenizer.
    pub fn new(source: &'a str) -> Self {
        Tokens { source, index: 0 }
    }
}

impl Iterator for Tokens<'_> {
    type Item = Result<Token, Error>;

    fn next(&mut self) -> Option<Result<Token, Error>> {
        let source = self.source.trim();
        if source.starts_with('0') {
            i64::from_str_radix(source, 8)
        } else {
            source.parse()
        }
        .map(|i| Token::Term(Term::Value(Value::Integer(i))))
        .map_err(|_| Error {
            cause: ErrorCause::InvalidCharacterInValue,
            location: 0..self.source.len(),
        })
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_integer_constants() {
        assert_eq!(
            Tokens::new("1").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(1)))))
        );
        assert_eq!(
            Tokens::new("42").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(42)))))
        );
    }

    #[test]
    fn invalid_digit_in_decimal_constant() {
        assert_eq!(
            Tokens::new("1a").next(),
            Some(Err(Error {
                cause: ErrorCause::InvalidCharacterInValue,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("123_456").next(),
            Some(Err(Error {
                cause: ErrorCause::InvalidCharacterInValue,
                location: 0..7,
            }))
        );
        // TODO Test with spaces
    }

    #[test]
    fn octal_integer_constants() {
        assert_eq!(
            Tokens::new("0").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(0)))))
        );
        assert_eq!(
            Tokens::new("01").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(1)))))
        );
        assert_eq!(
            Tokens::new("07").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(7)))))
        );
        assert_eq!(
            Tokens::new("0123").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(0o123)))))
        );
    }

    #[test]
    fn invalid_digit_in_octal_constant() {
        assert_eq!(
            Tokens::new("08").next(),
            Some(Err(Error {
                cause: ErrorCause::InvalidCharacterInValue,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("0192").next(),
            Some(Err(Error {
                cause: ErrorCause::InvalidCharacterInValue,
                location: 0..4,
            }))
        );
        assert_eq!(
            Tokens::new("0ab").next(),
            Some(Err(Error {
                cause: ErrorCause::InvalidCharacterInValue,
                location: 0..3,
            }))
        );
        // TODO Test with spaces
    }

    // TODO Hexadecimal integer constants
    // TODO Float constants
    // TODO Variables

    #[test]
    fn space_around_token() {
        assert_eq!(
            Tokens::new(" 42").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(42)))))
        );
        assert_eq!(
            Tokens::new("042 ").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(0o42)))))
        );
        assert_eq!(
            Tokens::new("\t 123 \n").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(123)))))
        );
    }
}
