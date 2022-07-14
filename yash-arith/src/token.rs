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
pub enum Token<'a> {
    /// Term
    Term(Term<'a>),
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

impl<'a> Iterator for Tokens<'a> {
    type Item = Result<Token<'a>, Error>;

    fn next(&mut self) -> Option<Result<Token<'a>, Error>> {
        let source = self.source[self.index..].trim_start();
        if source.is_empty() {
            return None;
        }
        let token_len = source
            .find(char::is_whitespace) // TODO Should delimit at an operator
            .unwrap_or(source.len());
        let token_source = &source[..token_len];
        let parse = if let Some(token_source) = token_source.strip_prefix("0X") {
            i64::from_str_radix(token_source, 0x10)
        } else if let Some(token_source) = token_source.strip_prefix("0x") {
            i64::from_str_radix(token_source, 0x10)
        } else if source.starts_with('0') {
            i64::from_str_radix(token_source, 8)
        } else {
            token_source.parse()
        };
        let start_of_token = self.source.len() - source.len();
        let end_of_token = start_of_token + token_len;
        match parse {
            Ok(i) => {
                self.index = end_of_token;
                Some(Ok(Token::Term(Term::Value(Value::Integer(i)))))
            }
            Err(_) => Some(Err(Error {
                cause: ErrorCause::InvalidNumericConstant,
                location: start_of_token..end_of_token,
            })),
        }
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
                cause: ErrorCause::InvalidNumericConstant,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("  123_456 ").next(),
            Some(Err(Error {
                cause: ErrorCause::InvalidNumericConstant,
                location: 2..9,
            }))
        );
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
                cause: ErrorCause::InvalidNumericConstant,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new(" 0192 ").next(),
            Some(Err(Error {
                cause: ErrorCause::InvalidNumericConstant,
                location: 1..5,
            }))
        );
        assert_eq!(
            Tokens::new("0ab").next(),
            Some(Err(Error {
                cause: ErrorCause::InvalidNumericConstant,
                location: 0..3,
            }))
        );
    }

    #[test]
    fn hexadecimal_integer_constants() {
        assert_eq!(
            Tokens::new("0x0").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(0x0)))))
        );
        assert_eq!(
            Tokens::new("0X1").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(0x1)))))
        );
        assert_eq!(
            Tokens::new("0x19Af").next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(0x19AF)))))
        );
    }

    #[test]
    fn broken_hexadecimal_integer_constants() {
        assert_eq!(
            Tokens::new("0x").next(),
            Some(Err(Error {
                cause: ErrorCause::InvalidNumericConstant,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new(" 0xG ").next(),
            Some(Err(Error {
                cause: ErrorCause::InvalidNumericConstant,
                location: 1..4,
            }))
        );
        assert_eq!(
            Tokens::new("0x1z2").next(),
            Some(Err(Error {
                cause: ErrorCause::InvalidNumericConstant,
                location: 0..5,
            }))
        );
    }

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

    #[test]
    fn parsing_two_tokens() {
        let mut tokens = Tokens::new(" 123  0456 ");
        assert_eq!(
            tokens.next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(123)))))
        );
        assert_eq!(
            tokens.next(),
            Some(Ok(Token::Term(Term::Value(Value::Integer(0o456)))))
        );
        assert_eq!(tokens.next(), None);
    }

    // TODO parsing_many_tokens "10.0e+3+0"
}
