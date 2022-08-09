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

use std::fmt::Display;
use std::ops::Range;

/// Result of evaluating an expression
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Value {
    Integer(i64),
    // TODO Float
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
pub enum Term<'a> {
    /// Value
    Value(Value),
    /// Variable
    Variable {
        /// Variable name
        name: &'a str,
        /// Range of the substring in the evaluated expression where the variable occurs
        location: Range<usize>,
    },
}

/// Operator
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Operator {
    /// `?`
    Question,
    /// `:`
    Colon,
    /// `|`
    Bar,
    /// `||`
    BarBar,
    /// `|=`
    BarEqual,
    /// `^`
    Caret,
    /// `^=`
    CaretEqual,
    /// `&`
    And,
    /// `&&`
    AndAnd,
    /// `&=`
    AndEqual,
    /// `=`
    Equal,
    /// `==`
    EqualEqual,
    /// `!`
    Bang,
    /// `!=`
    BangEqual,
    /// `<`
    Less,
    /// `<=`
    LessEqual,
    /// `<<`
    LessLess,
    /// `<<=`
    LessLessEqual,
    /// `>`
    Greater,
    /// `>=`
    GreaterEqual,
    /// `>>`
    GreaterGreater,
    /// `>>=`
    GreaterGreaterEqual,
    /// `+`
    Plus,
    /// `++`
    PlusPlus,
    /// `+=`
    PlusEqual,
    /// `-`
    Minus,
    /// `--`
    MinusMinus,
    /// `-=`
    MinusEqual,
    /// `*`
    Asterisk,
    /// `*=`
    AsteriskEqual,
    /// `/`
    Slash,
    /// `/=`
    SlashEqual,
    /// `%`
    Percent,
    /// `%=`
    PercentEqual,
    /// `~`
    Tilde,
    /// `(`
    OpenParen,
    /// `)`
    CloseParen,
}

/// Atomic lexical element of an expression
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TokenValue<'a> {
    /// Term
    Term(Term<'a>),
    /// Operator
    Operator {
        /// Operator kind
        operator: Operator,
        /// Range of the substring where the token occurs in the parsed expression
        location: Range<usize>,
    },
}

/// Cause of a tokenization error
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TokenError {
    /// A value token contains an invalid character.
    InvalidNumericConstant,
    /// An expression contains a character that is not a whitespace, number, or
    /// number.
    InvalidCharacter,
}

impl Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::InvalidNumericConstant => "invalid numeric constant".fmt(f),
            TokenError::InvalidCharacter => "invalid character".fmt(f),
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

/// List of all the operators.
///
/// If a prefix of a valid operator is another operator, the prefix (the shorter
/// operator) must appear after the longer. With this ordering, we can
/// short-circuit unnecessary matching on finding a first match.
const OPERATORS: &[(&str, Operator)] = &[
    ("?", Operator::Question),
    (":", Operator::Colon),
    ("|=", Operator::BarEqual),
    ("||", Operator::BarBar),
    ("|", Operator::Bar),
    ("^=", Operator::CaretEqual),
    ("^", Operator::Caret),
    ("&=", Operator::AndEqual),
    ("&&", Operator::AndAnd),
    ("&", Operator::And),
    ("==", Operator::EqualEqual),
    ("=", Operator::Equal),
    ("!=", Operator::BangEqual),
    ("<=", Operator::LessEqual),
    ("<<=", Operator::LessLessEqual),
    ("<<", Operator::LessLess),
    ("<", Operator::Less),
    (">=", Operator::GreaterEqual),
    (">>=", Operator::GreaterGreaterEqual),
    (">>", Operator::GreaterGreater),
    (">", Operator::Greater),
    ("+=", Operator::PlusEqual),
    ("++", Operator::PlusPlus),
    ("+", Operator::Plus),
    ("-=", Operator::MinusEqual),
    ("--", Operator::MinusMinus),
    ("-", Operator::Minus),
    ("*=", Operator::AsteriskEqual),
    ("*", Operator::Asterisk),
    ("/=", Operator::SlashEqual),
    ("/", Operator::Slash),
    ("%=", Operator::PercentEqual),
    ("%", Operator::Percent),
    ("~", Operator::Tilde),
    ("!", Operator::Bang),
    ("(", Operator::OpenParen),
    (")", Operator::CloseParen),
];

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
    type Item = Result<TokenValue<'a>, Error>;

    fn next(&mut self) -> Option<Result<TokenValue<'a>, Error>> {
        let source = self.source[self.index..].trim_start();
        let start_of_token = self.source.len() - source.len();
        let first_char = source.chars().next()?;

        if let Some((lexeme, operator)) = OPERATORS
            .iter()
            .copied()
            .find(|&(lexeme, _)| source.starts_with(lexeme))
        {
            // Okay, this is an operator.
            let end_of_token = start_of_token + lexeme.len();
            let location = start_of_token..end_of_token;
            self.index = end_of_token;
            Some(Ok(TokenValue::Operator { operator, location }))
        } else {
            // The next token should be a term. Try parsing it.
            let remainder = source.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
            let token_len = source.len() - remainder.len();
            if token_len == 0 {
                return Some(Err(Error {
                    cause: TokenError::InvalidCharacter,
                    location: start_of_token..start_of_token + 1,
                }));
            }
            let end_of_token = start_of_token + token_len;
            let location = start_of_token..end_of_token;
            let token = &source[..token_len];
            let term = if first_char.is_ascii_digit() {
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
                    Ok(i) => Term::Value(Value::Integer(i)),
                    Err(_) => {
                        return Some(Err(Error {
                            cause: TokenError::InvalidNumericConstant,
                            location,
                        }))
                    }
                }
            } else {
                Term::Variable {
                    name: token,
                    location,
                }
            };

            self.index = end_of_token;
            Some(Ok(TokenValue::Term(term)))
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
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(1)))))
        );
        assert_eq!(
            Tokens::new("42").next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(42)))))
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
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(0)))))
        );
        assert_eq!(
            Tokens::new("01").next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(0o1)))))
        );
        assert_eq!(
            Tokens::new("07").next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(0o7)))))
        );
        assert_eq!(
            Tokens::new("0123").next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(0o123)))))
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
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(0x0)))))
        );
        assert_eq!(
            Tokens::new("0X1").next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(0x1)))))
        );
        assert_eq!(
            Tokens::new("0x19Af").next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(0x19AF)))))
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
            Some(Ok(TokenValue::Term(Term::Variable {
                name: "abc",
                location: 0..3
            })))
        );
        assert_eq!(
            Tokens::new("foo_BAR").next(),
            Some(Ok(TokenValue::Term(Term::Variable {
                name: "foo_BAR",
                location: 0..7
            })))
        );
        assert_eq!(
            Tokens::new("a1B2c").next(),
            Some(Ok(TokenValue::Term(Term::Variable {
                name: "a1B2c",
                location: 0..5
            })))
        );
        assert_eq!(
            Tokens::new(" _var").next(),
            Some(Ok(TokenValue::Term(Term::Variable {
                name: "_var",
                location: 1..5
            })))
        );
    }

    #[test]
    fn operators() {
        assert_eq!(
            Tokens::new("?").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Question,
                location: 0..1
            }))
        );
        assert_eq!(
            Tokens::new(":").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Colon,
                location: 0..1
            }))
        );
        assert_eq!(
            Tokens::new("|").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Bar,
                location: 0..1
            }))
        );
        assert_eq!(
            Tokens::new("||").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::BarBar,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("|=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::BarEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("^").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Caret,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("^=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::CaretEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("&").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::And,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("&&").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::AndAnd,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("&=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::AndEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Equal,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("==").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::EqualEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("!=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::BangEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("<").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Less,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("<=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::LessEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("<<").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::LessLess,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("<<=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::LessLessEqual,
                location: 0..3,
            }))
        );
        assert_eq!(
            Tokens::new(">").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Greater,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new(">=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::GreaterEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new(">>").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::GreaterGreater,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new(">>=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::GreaterGreaterEqual,
                location: 0..3,
            }))
        );
        assert_eq!(
            Tokens::new("+").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Plus,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("++").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::PlusPlus,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("+=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::PlusEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("-").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Minus,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("--").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::MinusMinus,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("-=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::MinusEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("*").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Asterisk,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("*=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::AsteriskEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("/").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Slash,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("/=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::SlashEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("%").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Percent,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("%=").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::PercentEqual,
                location: 0..2,
            }))
        );
        assert_eq!(
            Tokens::new("~").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Tilde,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("!").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Bang,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new("(").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::OpenParen,
                location: 0..1
            }))
        );
        assert_eq!(
            Tokens::new("(").next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::OpenParen,
                location: 0..1
            }))
        );
    }

    #[test]
    fn space_around_token() {
        assert_eq!(
            Tokens::new(" 42").next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(42)))))
        );
        assert_eq!(
            Tokens::new("042 ").next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(0o42)))))
        );
        assert_eq!(
            Tokens::new("\t 123 \n").next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(123)))))
        );
    }

    #[test]
    fn parsing_two_tokens() {
        let mut tokens = Tokens::new(" 123  foo ");
        assert_eq!(
            tokens.next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(123)))))
        );
        assert_eq!(
            tokens.next(),
            Some(Ok(TokenValue::Term(Term::Variable {
                name: "foo",
                location: 6..9,
            })))
        );
        assert_eq!(tokens.next(), None);
    }

    #[test]
    fn parsing_many_tokens() {
        // TODO "10.0e+3+0"
        let mut tokens = Tokens::new(" 10+0 ");
        assert_eq!(
            tokens.next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(10)))))
        );
        assert_eq!(
            tokens.next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Plus,
                location: 3..4,
            }))
        );
        assert_eq!(
            tokens.next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(0)))))
        );
        assert_eq!(tokens.next(), None);
    }

    #[test]
    fn parsing_adjacent_operators() {
        let mut tokens = Tokens::new("+-0");
        assert_eq!(
            tokens.next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Plus,
                location: 0..1,
            }))
        );
        assert_eq!(
            tokens.next(),
            Some(Ok(TokenValue::Operator {
                operator: Operator::Minus,
                location: 1..2,
            }))
        );
        assert_eq!(
            tokens.next(),
            Some(Ok(TokenValue::Term(Term::Value(Value::Integer(0)))))
        );
        assert_eq!(tokens.next(), None);
    }

    #[test]
    fn unrecognized_character() {
        assert_eq!(
            Tokens::new("#").next(),
            Some(Err(Error {
                cause: TokenError::InvalidCharacter,
                location: 0..1,
            }))
        );
        assert_eq!(
            Tokens::new(" @@").next(),
            Some(Err(Error {
                cause: TokenError::InvalidCharacter,
                location: 1..2,
            }))
        );
    }
}
