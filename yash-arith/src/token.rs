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

use super::Term;
use super::Value;
use std::fmt::Display;
use std::ops::Range;

/// Operator
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Operator {
    /// `|`
    Bar,
    /// `||`
    BarBar,
    /// `^`
    Caret,
    /// `&`
    And,
    /// `&&`
    AndAnd,
    /// `==`
    EqualEqual,
    /// `!=`
    BangEqual,
    /// `<`
    Less,
    /// `<=`
    LessEqual,
    /// `<<`
    LessLess,
    /// `>`
    Greater,
    /// `>=`
    GreaterEqual,
    /// `>>`
    GreaterGreater,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Asterisk,
    /// `/`
    Slash,
    /// `%`
    Percent,
}

impl Operator {
    /// Returns the precedence of the operator.
    ///
    /// If the operator acts as both a unary and binary operator, the result is
    /// the precedence as a binary operator.
    pub fn precedence(self) -> u8 {
        use Operator::*;
        match self {
            BarBar => 2,
            AndAnd => 3,
            Bar => 4,
            Caret => 5,
            And => 6,
            EqualEqual | BangEqual => 7,
            Less | LessEqual | Greater | GreaterEqual => 8,
            LessLess | GreaterGreater => 9,
            Plus | Minus => 10,
            Asterisk | Slash | Percent => 11,
        }
    }
}

/// Value of a token
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TokenValue<'a> {
    /// Term
    Term(Term<'a>),
    /// Operator
    Operator(Operator),
}

/// Atomic lexical element of an expression
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Token<'a> {
    /// Value of the token
    pub value: TokenValue<'a>,
    /// Range of the substring where the token occurs in the parsed expression
    pub location: Range<usize>,
}

/// Cause of a tokenization error
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TokenError {
    /// A value expression contains an invalid character.
    InvalidNumericConstant,
}

impl Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::InvalidNumericConstant => "invalid numeric constant".fmt(f),
        }
    }
}

/// Description of an error that occurred during expansion
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Error {
    /// Cause of the error
    pub cause: TokenError,
    /// Range of the substring in the evaluated expression string where the error occurred
    pub location: Range<usize>,
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
        let start_of_token = self.source.len() - source.len();

        let mut chars = source.chars();
        let (result, token_len) = match chars.next() {
            None => return None,
            Some('|') => match chars.next() {
                Some('|') => (Ok(TokenValue::Operator(Operator::BarBar)), 2),
                _ => (Ok(TokenValue::Operator(Operator::Bar)), 1),
            },
            Some('^') => (Ok(TokenValue::Operator(Operator::Caret)), 1),
            Some('&') => match chars.next() {
                Some('&') => (Ok(TokenValue::Operator(Operator::AndAnd)), 2),
                _ => (Ok(TokenValue::Operator(Operator::And)), 1),
            },
            Some('=') => match chars.next() {
                Some('=') => (Ok(TokenValue::Operator(Operator::EqualEqual)), 2),
                c => todo!("unrecognized character {:?}", c),
            },
            Some('!') => match chars.next() {
                Some('=') => (Ok(TokenValue::Operator(Operator::BangEqual)), 2),
                c => todo!("unrecognized character {:?}", c),
            },
            Some('<') => match chars.next() {
                Some('=') => (Ok(TokenValue::Operator(Operator::LessEqual)), 2),
                Some('<') => (Ok(TokenValue::Operator(Operator::LessLess)), 2),
                _ => (Ok(TokenValue::Operator(Operator::Less)), 1),
            },
            Some('>') => match chars.next() {
                Some('=') => (Ok(TokenValue::Operator(Operator::GreaterEqual)), 2),
                Some('>') => (Ok(TokenValue::Operator(Operator::GreaterGreater)), 2),
                _ => (Ok(TokenValue::Operator(Operator::Greater)), 1),
            },
            Some('+') => (Ok(TokenValue::Operator(Operator::Plus)), 1),
            Some('-') => (Ok(TokenValue::Operator(Operator::Minus)), 1),
            Some('*') => (Ok(TokenValue::Operator(Operator::Asterisk)), 1),
            Some('/') => (Ok(TokenValue::Operator(Operator::Slash)), 1),
            Some('%') => (Ok(TokenValue::Operator(Operator::Percent)), 1),
            Some(c) if c.is_alphanumeric() => {
                let remainder =
                    source.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
                let token_len = source.len() - remainder.len();
                let token = &source[..token_len];
                let result = if c.is_ascii_digit() {
                    let parse = if let Some(token_source) = token.strip_prefix("0X") {
                        i64::from_str_radix(token_source, 0x10)
                    } else if let Some(token_source) = token.strip_prefix("0x") {
                        i64::from_str_radix(token_source, 0x10)
                    } else if source.starts_with('0') {
                        i64::from_str_radix(token, 0o10)
                    } else {
                        token.parse()
                    };
                    match parse {
                        Ok(i) => Ok(TokenValue::Term(Term::Value(Value::Integer(i)))),
                        Err(_) => Err(TokenError::InvalidNumericConstant),
                    }
                } else {
                    Ok(TokenValue::Term(Term::Variable(token)))
                };
                (result, token_len)
            }
            Some(c) => todo!("unrecognized character {:?}", c),
        };

        assert!(token_len > 0, "token should not be empty");
        let end_of_token = start_of_token + token_len;
        let location = start_of_token..end_of_token;
        self.index = end_of_token;

        Some(match result {
            Ok(value) => Ok(Token { value, location }),
            Err(cause) => Err(Error { cause, location }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_integer_constants() {
        assert_eq!(
            Tokens::new("1").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(1))),
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("42").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(42))),
                location: 0..2,
            }))
        );
    }

    #[test]
    fn invalid_digit_in_decimal_constant() {
        assert_eq!(
            Tokens::new("1a").next(),
            Some(Err(Error {
                cause: TokenError::InvalidNumericConstant,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("  123_456 ").next(),
            Some(Err(Error {
                cause: TokenError::InvalidNumericConstant,
                location: 2..9,
            }))
        );
    }

    #[test]
    fn octal_integer_constants() {
        assert_eq!(
            Tokens::new("0").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(0))),
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("01").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(1))),
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("07").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(7))),
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("0123").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(0o123))),
                location: 0..4,
            }))
        );
    }

    #[test]
    fn invalid_digit_in_octal_constant() {
        assert_eq!(
            Tokens::new("08").next(),
            Some(Err(Error {
                cause: TokenError::InvalidNumericConstant,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new(" 0192 ").next(),
            Some(Err(Error {
                cause: TokenError::InvalidNumericConstant,
                location: 1..5,
            }))
        );
        assert_eq!(
            Tokens::new("0ab").next(),
            Some(Err(Error {
                cause: TokenError::InvalidNumericConstant,
                location: 0..3,
            }))
        );
    }

    #[test]
    fn hexadecimal_integer_constants() {
        assert_eq!(
            Tokens::new("0x0").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(0x0))),
                location: 0..3,
            }))
        );
        assert_eq!(
            Tokens::new("0X1").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(0x1))),
                location: 0..3,
            }))
        );
        assert_eq!(
            Tokens::new("0x19Af").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(0x19AF))),
                location: 0..6,
            }))
        );
    }

    #[test]
    fn broken_hexadecimal_integer_constants() {
        assert_eq!(
            Tokens::new("0x").next(),
            Some(Err(Error {
                cause: TokenError::InvalidNumericConstant,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new(" 0xG ").next(),
            Some(Err(Error {
                cause: TokenError::InvalidNumericConstant,
                location: 1..4,
            }))
        );
        assert_eq!(
            Tokens::new("0x1z2").next(),
            Some(Err(Error {
                cause: TokenError::InvalidNumericConstant,
                location: 0..5,
            }))
        );
    }

    // TODO Float constants

    #[test]
    fn variables() {
        assert_eq!(
            Tokens::new("abc").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Variable("abc")),
                location: 0..3,
            }))
        );
        assert_eq!(
            Tokens::new("foo_BAR").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Variable("foo_BAR")),
                location: 0..7,
            }))
        );
        assert_eq!(
            Tokens::new("a1B2c").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Variable("a1B2c")),
                location: 0..5,
            }))
        );
    }

    #[test]
    fn operators() {
        assert_eq!(
            Tokens::new("|").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Bar),
                location: 0..1
            })),
        );
        assert_eq!(
            Tokens::new("||").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::BarBar),
                location: 0..2
            })),
        );
        assert_eq!(
            Tokens::new("^").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Caret),
                location: 0..1
            })),
        );
        assert_eq!(
            Tokens::new("&").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::And),
                location: 0..1
            })),
        );
        assert_eq!(
            Tokens::new("&&").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::AndAnd),
                location: 0..2
            })),
        );
        assert_eq!(
            Tokens::new("==").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::EqualEqual),
                location: 0..2
            })),
        );
        assert_eq!(
            Tokens::new("!=").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::BangEqual),
                location: 0..2
            })),
        );
        assert_eq!(
            Tokens::new("<").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Less),
                location: 0..1
            })),
        );
        assert_eq!(
            Tokens::new("<=").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::LessEqual),
                location: 0..2
            })),
        );
        assert_eq!(
            Tokens::new("<<").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::LessLess),
                location: 0..2
            })),
        );
        assert_eq!(
            Tokens::new(">").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Greater),
                location: 0..1
            })),
        );
        assert_eq!(
            Tokens::new(">=").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::GreaterEqual),
                location: 0..2
            })),
        );
        assert_eq!(
            Tokens::new(">>").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::GreaterGreater),
                location: 0..2
            })),
        );
        assert_eq!(
            Tokens::new("+").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Plus),
                location: 0..1
            })),
        );
        assert_eq!(
            Tokens::new("-").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Minus),
                location: 0..1
            })),
        );
        assert_eq!(
            Tokens::new("*").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Asterisk),
                location: 0..1
            })),
        );
        assert_eq!(
            Tokens::new("/").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Slash),
                location: 0..1
            })),
        );
        assert_eq!(
            Tokens::new("%").next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Percent),
                location: 0..1
            })),
        );
    }

    #[test]
    fn space_around_token() {
        assert_eq!(
            Tokens::new(" 42").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(42))),
                location: 1..3,
            }))
        );
        assert_eq!(
            Tokens::new("042 ").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(0o42))),
                location: 0..3,
            }))
        );
        assert_eq!(
            Tokens::new("\t 123 \n").next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(123))),
                location: 2..5,
            }))
        );
    }

    #[test]
    fn parsing_two_tokens() {
        let mut tokens = Tokens::new(" 123  foo ");
        assert_eq!(
            tokens.next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(123))),
                location: 1..4,
            })),
        );
        assert_eq!(
            tokens.next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Variable("foo")),
                location: 6..9,
            })),
        );
        assert_eq!(tokens.next(), None);
    }

    #[test]
    fn parsing_many_tokens() {
        // TODO "10.0e+3+0"
        let mut tokens = Tokens::new(" 10+0 ");
        assert_eq!(
            tokens.next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(10))),
                location: 1..3,
            })),
        );
        assert_eq!(
            tokens.next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Plus),
                location: 3..4,
            })),
        );
        assert_eq!(
            tokens.next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(0))),
                location: 4..5,
            })),
        );
        assert_eq!(tokens.next(), None);
    }

    #[test]
    fn parsing_adjacent_operators() {
        let mut tokens = Tokens::new("+-0");
        assert_eq!(
            tokens.next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Plus),
                location: 0..1,
            })),
        );
        assert_eq!(
            tokens.next(),
            Some(Ok(Token {
                value: TokenValue::Operator(Operator::Minus),
                location: 1..2,
            })),
        );
        assert_eq!(
            tokens.next(),
            Some(Ok(Token {
                value: TokenValue::Term(Term::Value(Value::Integer(0))),
                location: 2..3,
            })),
        );
        assert_eq!(tokens.next(), None);
    }
}
