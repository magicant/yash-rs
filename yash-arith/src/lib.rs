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
pub enum Term<'a> {
    /// Value
    Value(Value),
    /// Variable
    Variable(&'a str),
}

mod token;

use token::Operator;
use token::Token;
pub use token::TokenError;
use token::TokenValue;
use token::Tokens;

/// Cause of an arithmetic expansion error
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ErrorCause<E> {
    /// Error in tokenization
    TokenError(TokenError),
    /// A variable value that is not a valid number
    InvalidVariableValue(String),
    /// Error assigning a variable value.
    AssignVariableError(E),
}

impl<E: Display> Display for ErrorCause<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCause::TokenError(e) => e.fmt(f),
            ErrorCause::InvalidVariableValue(v) => {
                write!(f, "variable value {:?} cannot be parsed as a number", v)
            }
            ErrorCause::AssignVariableError(e) => e.fmt(f),
        }
    }
}

impl<E> From<TokenError> for ErrorCause<E> {
    fn from(e: TokenError) -> Self {
        ErrorCause::TokenError(e)
    }
}

/// Description of an error that occurred during expansion
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Error<E> {
    /// Cause of the error
    pub cause: ErrorCause<E>,
    /// Range of the substring in the evaluated expression string where the error occurred
    pub location: Range<usize>,
}

impl<E: Display> Display for Error<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}

impl<E: std::fmt::Debug + Display> std::error::Error for Error<E> {}

impl<E> From<token::Error> for Error<E> {
    fn from(e: token::Error) -> Self {
        Error {
            cause: e.cause.into(),
            location: e.location,
        }
    }
}

mod env;

pub use env::Env;

/// Expands a variable to its value.
fn expand_variable<E: Env>(
    name: &str,
    location: &Range<usize>,
    env: &E,
) -> Result<Value, Error<E::AssignVariableError>> {
    match env.get_variable(name) {
        Some(value) => match value.parse() {
            Ok(number) => Ok(Value::Integer(number)),
            Err(_) => Err(Error {
                cause: ErrorCause::InvalidVariableValue(value.to_string()),
                location: location.clone(),
            }),
        },
        None => Ok(Value::Integer(0)),
    }
}

/// Evaluates a leaf expression.
///
/// A leaf expression is a constant number, variable, or parenthesized
/// expression, optionally modified by a unary operator.
fn eval_leaf<E: Env>(
    tokens: &mut Tokens,
    env: &mut E,
) -> Result<Value, Error<E::AssignVariableError>> {
    match tokens.next().transpose()? {
        Some(Token {
            value: TokenValue::Term(Term::Value(value)),
            location: _,
        }) => Ok(value),

        Some(Token {
            value: TokenValue::Term(Term::Variable(name)),
            location,
        }) => expand_variable(name, &location, env),

        Some(Token {
            value: TokenValue::Operator(_op),
            location: _,
        }) => todo!("handle orphan operator"),

        None => todo!("handle missing token"),
    }
}

/// Performs arithmetic expansion
pub fn eval<E: Env>(expression: &str, env: &mut E) -> Result<Value, Error<E::AssignVariableError>> {
    let mut tokens = Tokens::new(expression);

    let mut value = eval_leaf(&mut tokens, env)?;

    while let Some(token) = tokens.next().transpose()? {
        match token {
            Token {
                value: TokenValue::Operator(Operator::Plus),
                location: _,
            } => {
                let rhs = eval_leaf(&mut tokens, env)?;
                let (Value::Integer(lhs), Value::Integer(rhs)) = (value, rhs);
                value = Value::Integer(lhs.checked_add(rhs).expect("todo: handle overflow"));
            }

            Token {
                value: TokenValue::Term(_),
                location: _,
            } => todo!("handle orphan term"),
        }
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn decimal_integer_constants() {
        let env = &mut HashMap::new();
        assert_eq!(eval("1", env), Ok(Value::Integer(1)));
        assert_eq!(eval("42", env), Ok(Value::Integer(42)));
    }

    #[test]
    fn octal_integer_constants() {
        let env = &mut HashMap::new();
        assert_eq!(eval("0", env), Ok(Value::Integer(0)));
        assert_eq!(eval("01", env), Ok(Value::Integer(1)));
        assert_eq!(eval("07", env), Ok(Value::Integer(7)));
        assert_eq!(eval("0123", env), Ok(Value::Integer(0o123)));
    }

    #[test]
    fn invalid_digit_in_octal_constant() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("08", env),
            Err(Error {
                cause: ErrorCause::TokenError(TokenError::InvalidNumericConstant),
                location: 0..2,
            })
        );
        assert_eq!(
            eval("0192", env),
            Err(Error {
                cause: ErrorCause::TokenError(TokenError::InvalidNumericConstant),
                location: 0..4,
            })
        );
    }

    #[test]
    fn space_around_token() {
        let env = &mut HashMap::new();
        assert_eq!(eval(" 12", env), Ok(Value::Integer(12)));
        assert_eq!(eval("12 ", env), Ok(Value::Integer(12)));
        assert_eq!(eval("\n 123 \t", env), Ok(Value::Integer(123)));
        // TODO Test with more complex expressions
    }

    #[test]
    fn unset_variable() {
        let env = &mut HashMap::new();
        assert_eq!(eval("foo", env), Ok(Value::Integer(0)));
        assert_eq!(eval("bar", env), Ok(Value::Integer(0)));
    }

    #[test]
    fn integer_variable() {
        let env = &mut HashMap::new();
        env.insert("foo".to_string(), "42".to_string());
        env.insert("bar".to_string(), "123".to_string());
        assert_eq!(eval("foo", env), Ok(Value::Integer(42)));
        assert_eq!(eval("bar", env), Ok(Value::Integer(123)));
    }

    // TODO Variables (floats, infinities, & NaNs)

    #[test]
    #[ignore]
    fn invalid_variable_value() {
        let env = &mut HashMap::new();
        env.insert("foo".to_string(), "".to_string());
        env.insert("bar".to_string(), "*".to_string());
        env.insert("oops".to_string(), "foo".to_string());
        assert_eq!(
            eval("foo", env),
            Err(Error {
                cause: ErrorCause::InvalidVariableValue("".to_string()),
                location: 0..3,
            })
        );
        assert_eq!(
            eval("bar", env),
            Err(Error {
                cause: ErrorCause::InvalidVariableValue("*".to_string()),
                location: 0..3,
            })
        );
        assert_eq!(
            eval("  oops ", env),
            Err(Error {
                cause: ErrorCause::InvalidVariableValue("foo".to_string()),
                location: 2..5,
            })
        );
    }

    #[test]
    fn addition_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("1+2", env), Ok(Value::Integer(3)));
        assert_eq!(eval(" 12 + 34 ", env), Ok(Value::Integer(46)));
        assert_eq!(eval(" 3 + 16 + 5 ", env), Ok(Value::Integer(24)));
    }

    // TODO Operators
}
