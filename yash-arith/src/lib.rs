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

use assert_matches::assert_matches;
use std::fmt::Display;
use std::iter::Peekable;
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
    Variable {
        /// Variable name
        name: &'a str,
        /// Range of the substring in the evaluated expression where the variable occurs
        location: Range<usize>,
    },
}

mod token;

use token::Operator;
use token::Token;
pub use token::TokenError;
use token::Tokens;

/// Cause of an arithmetic expansion error
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ErrorCause<E> {
    /// Error in tokenization
    TokenError(TokenError),
    /// A variable value that is not a valid number
    InvalidVariableValue(String),
    /// Result out of bounds
    Overflow,
    /// Division by zero
    DivisionByZero,
    /// Error assigning a variable value.
    AssignVariableError(E),
}

impl<E: Display> Display for ErrorCause<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ErrorCause::*;
        match self {
            TokenError(e) => e.fmt(f),
            InvalidVariableValue(v) => {
                write!(f, "variable value {:?} cannot be parsed as a number", v)
            }
            Overflow => "overflow".fmt(f),
            DivisionByZero => "division by zero".fmt(f),
            AssignVariableError(e) => e.fmt(f),
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
fn eval_leaf<'a, E: Env>(
    tokens: &mut Peekable<Tokens<'a>>,
    env: &mut E,
) -> Result<Term<'a>, Error<E::AssignVariableError>> {
    match tokens.next().transpose()? {
        // TODO Don't expand variable here
        Some(Token::Term(Term::Variable { name, location })) => {
            expand_variable(name, &location, env).map(Term::Value)
        }
        Some(Token::Term(term)) => Ok(term),
        Some(Token::Operator { .. }) => todo!("handle orphan operator"),
        None => todo!("handle missing token"),
    }
}

fn unwrap_or_overflow<T, E>(result: Option<T>, location: Range<usize>) -> Result<T, Error<E>> {
    result.ok_or(Error {
        cause: ErrorCause::Overflow,
        location,
    })
}

/// Evaluates an expression that may contain binary operators.
///
/// This function consumes binary operators with precedence equal to or greater
/// than the given minimum precedence.
fn eval_binary<'a, E: Env>(
    tokens: &mut Peekable<Tokens<'a>>,
    min_precedence: u8,
    env: &mut E,
) -> Result<Term<'a>, Error<E::AssignVariableError>> {
    let mut term = eval_leaf(tokens, env)?;

    while let Some(&Ok(Token::Operator { operator, .. })) = tokens.peek() {
        let precedence = operator.precedence();
        if precedence < min_precedence {
            break;
        }

        let location =
            assert_matches!(tokens.next(), Some(Ok(Token::Operator { location, .. })) => location);
        let rhs = eval_binary(tokens, precedence + 1, env)?;
        let (Value::Integer(lhs), Value::Integer(rhs)) = match (term, rhs) {
            (Term::Value(lhs), Term::Value(rhs)) => (lhs, rhs),
            _ => todo!(),
        };
        use Operator::*;
        term = Term::Value(match operator {
            BarBar => Value::Integer((lhs != 0 || rhs != 0) as _),
            AndAnd => Value::Integer((lhs != 0 && rhs != 0) as _),
            Bar => Value::Integer(lhs | rhs),
            Caret => Value::Integer(lhs ^ rhs),
            And => Value::Integer(lhs & rhs),
            EqualEqual => Value::Integer((lhs == rhs) as _),
            BangEqual => Value::Integer((lhs != rhs) as _),
            Less => Value::Integer((lhs < rhs) as _),
            Greater => Value::Integer((lhs > rhs) as _),
            LessEqual => Value::Integer((lhs <= rhs) as _),
            GreaterEqual => Value::Integer((lhs >= rhs) as _),
            LessLess => {
                let rhs = unwrap_or_overflow(rhs.try_into().ok(), location.clone())?;
                let result = unwrap_or_overflow(lhs.checked_shl(rhs), location.clone())?;
                if result >> rhs != lhs {
                    return Err(Error {
                        cause: ErrorCause::Overflow,
                        location,
                    });
                }
                Value::Integer(result)
            }
            GreaterGreater => Value::Integer(unwrap_or_overflow(
                rhs.try_into().ok().and_then(|rhs| lhs.checked_shr(rhs)),
                location,
            )?),
            Plus => Value::Integer(unwrap_or_overflow(lhs.checked_add(rhs), location)?),
            Minus => Value::Integer(unwrap_or_overflow(lhs.checked_sub(rhs), location)?),
            Asterisk => Value::Integer(unwrap_or_overflow(lhs.checked_mul(rhs), location)?),
            Slash => {
                if rhs == 0 {
                    return Err(Error {
                        cause: ErrorCause::DivisionByZero,
                        location,
                    });
                } else {
                    Value::Integer(unwrap_or_overflow(lhs.checked_div(rhs), location)?)
                }
            }
            Percent => {
                if rhs == 0 {
                    return Err(Error {
                        cause: ErrorCause::DivisionByZero,
                        location,
                    });
                } else {
                    Value::Integer(unwrap_or_overflow(lhs.checked_rem(rhs), location)?)
                }
            }
        });
    }

    Ok(term)
}

/// Performs arithmetic expansion
pub fn eval<E: Env>(expression: &str, env: &mut E) -> Result<Value, Error<E::AssignVariableError>> {
    let mut tokens = Tokens::new(expression).peekable();
    let term = eval_binary(&mut tokens, 0, env)?;
    assert_eq!(tokens.next(), None, "todo: handle orphan term");
    match term {
        Term::Value(value) => Ok(value),
        Term::Variable { .. } => todo!("expand variable"),
    }
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
                location: 2..6,
            })
        );
    }

    #[test]
    fn boolean_logic_operators() {
        let env = &mut HashMap::new();
        assert_eq!(eval("0||0", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 1 || 0 ", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 0 || 1 ", env), Ok(Value::Integer(1)));
        assert_eq!(eval("2 || 3", env), Ok(Value::Integer(1)));

        assert_eq!(eval("0&&0", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 1 && 0 ", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 0 && 1 ", env), Ok(Value::Integer(0)));
        assert_eq!(eval("2 && 3", env), Ok(Value::Integer(1)));
    }

    // TODO conditional_evaluation_in_boolean_logic_operators

    #[test]
    fn bitwise_logic_operators() {
        let env = &mut HashMap::new();
        assert_eq!(eval("3|5", env), Ok(Value::Integer(7)));
        assert_eq!(eval(" 5 | 3 ", env), Ok(Value::Integer(7)));
        assert_eq!(eval(" 10 | 10 ", env), Ok(Value::Integer(10)));
        assert_eq!(eval(" 7 | 14 | 28 ", env), Ok(Value::Integer(31)));

        assert_eq!(eval("3^5", env), Ok(Value::Integer(6)));
        assert_eq!(eval(" 5 ^ 3 ", env), Ok(Value::Integer(6)));
        assert_eq!(eval(" 10 ^ 10 ", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 7 ^ 14 ^ 28 ", env), Ok(Value::Integer(21)));

        assert_eq!(eval("3&5", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 5 & 3 ", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 10 & 10 ", env), Ok(Value::Integer(10)));
        assert_eq!(eval(" 7 & 14 & 28 ", env), Ok(Value::Integer(4)));
    }

    #[test]
    fn equality_comparison_operators() {
        let env = &mut HashMap::new();
        assert_eq!(eval("1==2", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 2 == 1 ", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 5 == 5 ", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 1 == 2 == 2 ", env), Ok(Value::Integer(0)));

        assert_eq!(eval("1!=2", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 2 != 1 ", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 5 != 5 ", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 1 != 1 != 2 ", env), Ok(Value::Integer(1)));
    }

    #[test]
    fn inequality_comparison_operators() {
        let env = &mut HashMap::new();
        assert_eq!(eval("1<2", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 2 < 1 ", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 5 < 5 ", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 3 < 3 < 3 ", env), Ok(Value::Integer(1)));

        assert_eq!(eval("1<=2", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 2 <= 1 ", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 5 <= 5 ", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 3 <= 3 <= 3 ", env), Ok(Value::Integer(1)));

        assert_eq!(eval("1>2", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 2 > 1 ", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 5 > 5 ", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 3 > 3 > 3 ", env), Ok(Value::Integer(0)));

        assert_eq!(eval("1>=2", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 2 >= 1 ", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 5 >= 5 ", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 3 >= 3 >= 3 ", env), Ok(Value::Integer(0)));
    }

    #[test]
    fn bit_shift_operators() {
        let env = &mut HashMap::new();
        assert_eq!(eval("5<<3", env), Ok(Value::Integer(40)));
        assert_eq!(eval(" 3 << 5 ", env), Ok(Value::Integer(96)));
        assert_eq!(eval(" 2 << 2 << 2 ", env), Ok(Value::Integer(32)));

        assert_eq!(eval("64>>3", env), Ok(Value::Integer(8)));
        assert_eq!(eval(" 63 >> 3 ", env), Ok(Value::Integer(7)));
        assert_eq!(eval(" 2 >> 2 >> 2 ", env), Ok(Value::Integer(0)));
    }

    #[test]
    fn overflow_in_bit_shifting() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("0x4000000000000000<<1", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 18..20,
            })
        );
        assert_eq!(
            eval("0<<1000", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 1..3,
            })
        );
        // TODO 1 << -1

        assert_eq!(
            eval("0>>1000", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 1..3,
            })
        );
        // TODO 1 >> -1
    }

    #[test]
    fn addition_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("1+2", env), Ok(Value::Integer(3)));
        assert_eq!(eval(" 12 + 34 ", env), Ok(Value::Integer(46)));
        assert_eq!(eval(" 3 + 16 + 5 ", env), Ok(Value::Integer(24)));
    }

    #[test]
    fn overflow_in_addition() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("9223372036854775807+1", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 19..20,
            })
        );
    }

    #[test]
    fn subtraction_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("2-1", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" 42 - 15 ", env), Ok(Value::Integer(27)));
        assert_eq!(eval(" 10 - 7 - 5 ", env), Ok(Value::Integer(-2)));
    }

    #[test]
    fn overflow_in_subtraction() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("0-9223372036854775807-2", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 21..22,
            })
        );
    }

    #[test]
    fn multiplication_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("3*6", env), Ok(Value::Integer(18)));
        assert_eq!(eval(" 5 * 11 ", env), Ok(Value::Integer(55)));
        assert_eq!(eval(" 2 * 3 * 4 ", env), Ok(Value::Integer(24)));
    }

    #[test]
    fn overflow_in_multiplication() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("0x100000000 * 0x80000000", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 12..13,
            })
        );
    }

    #[test]
    fn division_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("6/2", env), Ok(Value::Integer(3)));
        assert_eq!(eval(" 120 / 24 ", env), Ok(Value::Integer(5)));
        assert_eq!(eval(" 120/10/5 ", env), Ok(Value::Integer(2)));
    }

    #[test]
    fn division_by_zero() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("1/0", env),
            Err(Error {
                cause: ErrorCause::DivisionByZero,
                location: 1..2,
            })
        );
        assert_eq!(
            eval("0/0", env),
            Err(Error {
                cause: ErrorCause::DivisionByZero,
                location: 1..2,
            })
        );
        assert_eq!(
            eval("10/0", env),
            Err(Error {
                cause: ErrorCause::DivisionByZero,
                location: 2..3,
            })
        );
    }

    // TODO overflow_in_division

    #[test]
    fn remainder_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("6%2", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" 17 % 5 ", env), Ok(Value::Integer(2)));
        assert_eq!(eval(" 42 % 11 % 5 ", env), Ok(Value::Integer(4)));
    }

    #[test]
    fn remainder_by_zero() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("1%0", env),
            Err(Error {
                cause: ErrorCause::DivisionByZero,
                location: 1..2,
            })
        );
        assert_eq!(
            eval("0%0", env),
            Err(Error {
                cause: ErrorCause::DivisionByZero,
                location: 1..2,
            })
        );
        assert_eq!(
            eval("10%0", env),
            Err(Error {
                cause: ErrorCause::DivisionByZero,
                location: 2..3,
            })
        );
    }

    // TODO overflow_in_remainder

    #[test]
    fn combining_operators_of_same_precedence() {
        let env = &mut HashMap::new();
        assert_eq!(eval("2+5-3", env), Ok(Value::Integer(4)));
    }

    #[test]
    fn combining_operators_of_different_precedences() {
        let env = &mut HashMap::new();
        assert_eq!(eval("2+3*4", env), Ok(Value::Integer(14)));
        assert_eq!(eval("2*3+4", env), Ok(Value::Integer(10)));
    }

    // TODO Operators
}
