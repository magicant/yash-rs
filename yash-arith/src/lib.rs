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

mod token;

use token::Operator;
use token::Term;
use token::Token;
pub use token::TokenError;
use token::Tokens;
pub use token::Value;

mod ast;

// TODO Consider making these public
// use ast::parse;
// use ast::Ast;
pub use ast::SyntaxError;

mod eval;

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
    /// Left bit-shifting of a negative value
    LeftShiftingNegative,
    /// Bit-shifting with a negative right-hand-side operand
    ReverseShifting,
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
            LeftShiftingNegative => "negative value cannot be left-shifted".fmt(f),
            ReverseShifting => "bit-shifting with negative right-hand-side operand".fmt(f),
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
    mode: Mode,
    env: &E,
) -> Result<Value, Error<E::AssignVariableError>> {
    if let (Mode::Eval, Some(value)) = (mode, env.get_variable(name)) {
        match value.parse() {
            Ok(number) => Ok(Value::Integer(number)),
            Err(_) => Err(Error {
                cause: ErrorCause::InvalidVariableValue(value.to_string()),
                location: location.clone(),
            }),
        }
    } else {
        Ok(Value::Integer(0))
    }
}

/// Specifies the behavior of parse functions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    /// Evaluate the (sub)expression parsed.
    Eval,
    /// Just parse a (sub)expression; don't evaluate.
    Skip,
}

impl Term<'_> {
    /// Evaluate the term into a value.
    fn into_value<E: Env>(
        self,
        mode: Mode,
        env: &E,
    ) -> Result<Value, Error<E::AssignVariableError>> {
        match mode {
            Mode::Eval => match self {
                Term::Value(value) => Ok(value),
                Term::Variable { name, location } => expand_variable(name, &location, mode, env),
            },
            Mode::Skip => Ok(Value::Integer(0)),
        }
    }
}

fn unwrap_or_overflow<T, E>(result: Option<T>, location: Range<usize>) -> Result<T, Error<E>> {
    result.ok_or(Error {
        cause: ErrorCause::Overflow,
        location,
    })
}

/// Applies a unary operator.
fn apply_unary<E>(
    operator: Operator,
    operand: Value,
    location: Range<usize>,
) -> Result<Value, Error<E>> {
    let Value::Integer(value) = operand;
    use Operator::*;
    Ok(match operator {
        Plus => Value::Integer(value),
        Minus => Value::Integer(unwrap_or_overflow(value.checked_neg(), location)?),
        Tilde => Value::Integer(!value),
        Bang => Value::Integer((value == 0) as i64),
        PlusPlus => Value::Integer(unwrap_or_overflow(value.checked_add(1), location)?),
        MinusMinus => Value::Integer(unwrap_or_overflow(value.checked_sub(1), location)?),
        _ => panic!("not a unary operator: {:?}", operator),
    })
}

/// Parses optional postfix operators.
fn parse_postfix<'a, E: Env>(
    operand: Term<'a>,
    tokens: &mut Peekable<Tokens<'a>>,
    mode: Mode,
    env: &mut E,
) -> Result<Term<'a>, Error<E::AssignVariableError>> {
    if let Some(Ok(Token::Operator {
        operator: Operator::PlusPlus | Operator::MinusMinus,
        ..
    })) = tokens.peek()
    {
        let (operator, op_location) = assert_matches!(
            tokens.next(),
            Some(Ok(Token::Operator { operator, location })) => (operator, location)
        );
        match operand {
            Term::Value(_) => todo!("reject non-variable"),
            Term::Variable { name, location } => {
                let old_value = expand_variable(name, &location, mode, env)?;
                let new_value = apply_unary(operator, old_value.clone(), op_location.clone())?;

                if mode == Mode::Eval {
                    env.assign_variable(name, new_value.to_string())
                        .map_err(|e| Error {
                            cause: ErrorCause::AssignVariableError(e),
                            location: op_location,
                        })?;
                }
                Ok(Term::Value(old_value))
            }
        }
    } else {
        Ok(operand)
    }
}

/// Parses a leaf expression.
///
/// A leaf expression is a constant number, variable, or parenthesized
/// expression, optionally modified by a unary operator.
fn parse_leaf<'a, E: Env>(
    tokens: &mut Peekable<Tokens<'a>>,
    mode: Mode,
    env: &mut E,
) -> Result<Term<'a>, Error<E::AssignVariableError>> {
    use Operator::*;
    match tokens.next().transpose()? {
        Some(Token::Term(term)) => parse_postfix(term, tokens, mode, env),

        Some(Token::Operator {
            operator,
            location: op_location,
        }) => match operator {
            OpenParen => {
                let inner = parse_binary(tokens, 1, mode, env)?;
                tokens.next().transpose()?; // TODO Check if this token is a closing parenthesis
                parse_postfix(inner, tokens, mode, env)
            }
            Plus | Minus | Tilde | Bang => {
                let operand = parse_leaf(tokens, mode, env)?.into_value(mode, env)?;
                apply_unary(operator, operand, op_location).map(Term::Value)
            }
            PlusPlus | MinusMinus => match parse_leaf(tokens, mode, env)? {
                Term::Value(_) => todo!("reject non-variable"),
                Term::Variable { name, location } => {
                    let old_value = expand_variable(name, &location, mode, env)?;
                    let result = if mode == Mode::Eval {
                        let new_value = apply_unary(operator, old_value, op_location.clone())?;
                        env.assign_variable(name, new_value.to_string())
                            .map_err(|e| Error {
                                cause: ErrorCause::AssignVariableError(e),
                                location: op_location,
                            })?;
                        new_value
                    } else {
                        Value::Integer(0)
                    };
                    Ok(Term::Value(result))
                }
            },
            _ => todo!("handle orphan operator: {:?}", operator),
        },

        None => todo!("handle missing token"),
    }
}

/// Applies a binary operator.
///
/// If `op` is a compound assignment operator, only the binary operation is
/// performed, ignoring the assignment. For `Operator::Equal`, `rhs` is
/// returned, ignoring `lhs`.
fn apply_binary<E>(
    op: Operator,
    lhs: Value,
    rhs: Value,
    location: Range<usize>,
) -> Result<Value, Error<E>> {
    let (Value::Integer(lhs), Value::Integer(rhs)) = (lhs, rhs);
    use Operator::*;
    Ok(match op {
        Equal => Value::Integer(rhs),
        BarBar => Value::Integer((lhs != 0 || rhs != 0) as _),
        AndAnd => Value::Integer((lhs != 0 && rhs != 0) as _),
        Bar | BarEqual => Value::Integer(lhs | rhs),
        Caret | CaretEqual => Value::Integer(lhs ^ rhs),
        And | AndEqual => Value::Integer(lhs & rhs),
        EqualEqual => Value::Integer((lhs == rhs) as _),
        BangEqual => Value::Integer((lhs != rhs) as _),
        Less => Value::Integer((lhs < rhs) as _),
        Greater => Value::Integer((lhs > rhs) as _),
        LessEqual => Value::Integer((lhs <= rhs) as _),
        GreaterEqual => Value::Integer((lhs >= rhs) as _),
        LessLess | LessLessEqual => {
            if lhs < 0 {
                return Err(Error {
                    cause: ErrorCause::LeftShiftingNegative,
                    location,
                });
            }
            if rhs < 0 {
                return Err(Error {
                    cause: ErrorCause::ReverseShifting,
                    location,
                });
            }
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
        GreaterGreater | GreaterGreaterEqual => {
            if rhs < 0 {
                return Err(Error {
                    cause: ErrorCause::ReverseShifting,
                    location,
                });
            }
            Value::Integer(unwrap_or_overflow(
                rhs.try_into().ok().and_then(|rhs| lhs.checked_shr(rhs)),
                location,
            )?)
        }
        Plus | PlusEqual => Value::Integer(unwrap_or_overflow(lhs.checked_add(rhs), location)?),
        Minus | MinusEqual => Value::Integer(unwrap_or_overflow(lhs.checked_sub(rhs), location)?),
        Asterisk | AsteriskEqual => {
            Value::Integer(unwrap_or_overflow(lhs.checked_mul(rhs), location)?)
        }
        Slash | SlashEqual => {
            if rhs == 0 {
                return Err(Error {
                    cause: ErrorCause::DivisionByZero,
                    location,
                });
            } else {
                Value::Integer(unwrap_or_overflow(lhs.checked_div(rhs), location)?)
            }
        }
        Percent | PercentEqual => {
            if rhs == 0 {
                return Err(Error {
                    cause: ErrorCause::DivisionByZero,
                    location,
                });
            } else {
                Value::Integer(unwrap_or_overflow(lhs.checked_rem(rhs), location)?)
            }
        }
        Question | Colon | Tilde | Bang | PlusPlus | MinusMinus | OpenParen | CloseParen => {
            panic!("not a binary operator: {:?}", op)
        }
    })
}

/// Parses an expression that may contain binary operators.
///
/// This function consumes binary operators with precedence equal to or greater
/// than the given minimum precedence, which must be greater than 0.
fn parse_binary<'a, E: Env>(
    tokens: &mut Peekable<Tokens<'a>>,
    min_precedence: u8,
    mode: Mode,
    env: &mut E,
) -> Result<Term<'a>, Error<E::AssignVariableError>> {
    let mut term = parse_leaf(tokens, mode, env)?;

    while let Some(&Ok(Token::Operator { operator, .. })) = tokens.peek() {
        let precedence = operator.precedence();
        if precedence < min_precedence {
            break;
        }

        let op_location =
            assert_matches!(tokens.next(), Some(Ok(Token::Operator { location, .. })) => location);

        use Operator::*;
        match operator {
            Equal | BarEqual | CaretEqual | AndEqual | LessLessEqual | GreaterGreaterEqual
            | PlusEqual | MinusEqual | AsteriskEqual | SlashEqual | PercentEqual => match term {
                Term::Value(_) => todo!(),
                Term::Variable { name, location } => {
                    let lhs = if operator == Equal {
                        Value::Integer(0)
                    } else {
                        expand_variable(name, &location, mode, env)?
                    };
                    let rhs = parse_binary(tokens, precedence, mode, env)?.into_value(mode, env)?;
                    let result = if mode == Mode::Eval {
                        let result = apply_binary(operator, lhs, rhs, op_location.clone())?;
                        env.assign_variable(name, result.to_string())
                            .map_err(|e| Error {
                                cause: ErrorCause::AssignVariableError(e),
                                location: op_location,
                            })?;
                        result
                    } else {
                        Value::Integer(0)
                    };
                    term = Term::Value(result);
                }
            },
            Question => {
                let Value::Integer(condition) = term.into_value(mode, env)?;
                let (then_mode, else_mode) = if condition != 0 {
                    (mode, Mode::Skip)
                } else {
                    (Mode::Skip, mode)
                };
                debug_assert_eq!(precedence, 2);
                let then_result = parse_binary(tokens, 1, then_mode, env)?;
                // TODO Reject if a colon is missing
                tokens.next().transpose()?;
                let else_result = parse_binary(tokens, 2, else_mode, env)?;
                term = if condition != 0 {
                    then_result
                } else {
                    else_result
                };
                // TODO result into_value
            }
            BarBar | AndAnd => {
                let Value::Integer(lhs) = term.into_value(mode, env)?;
                let skip_rhs = match operator {
                    BarBar => lhs != 0,
                    AndAnd => lhs == 0,
                    _ => unreachable!(),
                };
                let rhs_mode = if skip_rhs { Mode::Skip } else { mode };
                let rhs = parse_binary(tokens, precedence + 1, rhs_mode, env)?
                    .into_value(rhs_mode, env)?;
                let value = apply_binary(operator, Value::Integer(lhs), rhs, op_location)?;
                term = Term::Value(value);
            }
            Bar | Caret | And | EqualEqual | BangEqual | Less | LessEqual | Greater
            | GreaterEqual | LessLess | GreaterGreater | Plus | Minus | Asterisk | Slash
            | Percent => {
                let rhs = parse_binary(tokens, precedence + 1, mode, env)?;
                let (lhs, rhs) = (term.into_value(mode, env)?, rhs.into_value(mode, env)?);
                term = Term::Value(apply_binary(operator, lhs, rhs, op_location)?);
            }
            Colon | Tilde | Bang | PlusPlus | MinusMinus | OpenParen => todo!("syntax error"),
            CloseParen => panic!("min_precedence must not be 0"),
        };
    }

    Ok(term)
}

/// Performs arithmetic expansion
pub fn eval<E: Env>(expression: &str, env: &mut E) -> Result<Value, Error<E::AssignVariableError>> {
    let mut tokens = Tokens::new(expression).peekable();
    let term = parse_binary(&mut tokens, 1, Mode::Eval, env)?;
    assert_eq!(tokens.next(), None, "todo: handle orphan term");
    term.into_value(Mode::Eval, env)
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
    fn unevaluated_variable_value() {
        let env = &mut HashMap::new();
        env.insert("empty".to_string(), "".to_string());
        assert_eq!(eval("1 || empty", env), Ok(Value::Integer(1)));
        assert_eq!(eval("0 && empty++", env), Ok(Value::Integer(0)));
        assert_eq!(eval("1 ? 2 : --empty", env), Ok(Value::Integer(2)));
        assert_eq!(eval("0 ? empty /= 1 : 3", env), Ok(Value::Integer(3)));
    }

    #[test]
    fn simple_assignment_operator() {
        let env = &mut HashMap::new();
        env.insert("foo".to_string(), "#ignored_value#".to_string());

        assert_eq!(eval("a=1", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" foo = 42 ", env), Ok(Value::Integer(42)));

        assert_eq!(env["a"], "1");
        assert_eq!(env["foo"], "42");
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn compound_assignment_operators() {
        let env = &mut HashMap::new();
        assert_eq!(eval("a|=1", env), Ok(Value::Integer(1)));
        assert_eq!(eval("a^=7", env), Ok(Value::Integer(6)));
        assert_eq!(eval("a&=3", env), Ok(Value::Integer(2)));
        assert_eq!(eval("a<<=4", env), Ok(Value::Integer(32)));
        assert_eq!(eval("a>>=2", env), Ok(Value::Integer(8)));
        assert_eq!(eval("a+=1", env), Ok(Value::Integer(9)));
        assert_eq!(eval("a-=4", env), Ok(Value::Integer(5)));
        assert_eq!(eval("a*=21", env), Ok(Value::Integer(105)));
        assert_eq!(eval("a/=8", env), Ok(Value::Integer(13)));
        assert_eq!(eval("a%=8", env), Ok(Value::Integer(5)));
        assert_eq!(env["a"], "5");
    }

    #[test]
    fn combining_assignment_operators() {
        let env = &mut HashMap::new();
        assert_eq!(eval("a = b -= c = 7", env), Ok(Value::Integer(-7)));
        assert_eq!(env["a"], "-7");
        assert_eq!(env["b"], "-7");
        assert_eq!(env["c"], "7");
    }

    #[test]
    fn conditional_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("1?a=10:(b=20)", env), Ok(Value::Integer(10)));
        assert_eq!(env["a"], "10");
        assert_eq!(env.get("b"), None);

        assert_eq!(eval("0 ? x = 30 : (y = 40)", env), Ok(Value::Integer(40)));
        assert_eq!(env.get("x"), None);
        assert_eq!(env["y"], "40");

        assert_eq!(eval("9 ? 1 : 0 ? 2 : 3", env), Ok(Value::Integer(1)));
        assert_eq!(eval("0 ? 1 : 0 ? 2 : 3", env), Ok(Value::Integer(3)));
    }

    #[test]
    fn conditional_evaluation_in_conditional_operators() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("1 ? 2 : (a = 3) ? b = 4 : (c = 5)", env),
            Ok(Value::Integer(2))
        );
        assert!(env.is_empty(), "expected empty env: {:?}", env);

        assert_eq!(
            eval("0 ? (a = 1) ? b = 2 : (c = 3) : 4", env),
            Ok(Value::Integer(4))
        );
        assert!(env.is_empty(), "expected empty env: {:?}", env);
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

    #[test]
    fn conditional_evaluation_in_boolean_logic_operators() {
        let env = &mut HashMap::new();
        assert_eq!(eval("(a = 0) || (b = 2)", env), Ok(Value::Integer(1)));
        assert_eq!(env["a"], "0");
        assert_eq!(env["b"], "2");

        let env = &mut HashMap::new();
        assert_eq!(eval("(a = 3) || (b = 2)", env), Ok(Value::Integer(1)));
        assert_eq!(env["a"], "3");
        assert_eq!(env.get("b"), None);

        let env = &mut HashMap::new();
        assert_eq!(eval("(a = 0) && (b = 2)", env), Ok(Value::Integer(0)));
        assert_eq!(env["a"], "0");
        assert_eq!(env.get("b"), None);

        let env = &mut HashMap::new();
        assert_eq!(eval("(a = 3) && (b = 2)", env), Ok(Value::Integer(1)));
        assert_eq!(env["a"], "3");
        assert_eq!(env["b"], "2");

        let env = &mut HashMap::new();
        env.insert("x".to_string(), "@".to_string());
        assert_eq!(eval("0 && (x || x)", env), Ok(Value::Integer(0)));
        assert_eq!(eval("1 || x && x", env), Ok(Value::Integer(1)));

        let env = &mut HashMap::new();
        assert_eq!(eval("0 && ++x", env), Ok(Value::Integer(0)));
        assert_eq!(env.get("x"), None);

        let env = &mut HashMap::new();
        assert_eq!(eval("0 && x++", env), Ok(Value::Integer(0)));
        assert_eq!(env.get("x"), None);
    }

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
        assert_eq!(
            eval("0<<0x100000000", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 1..3,
            })
        );

        assert_eq!(
            eval("0>>1000", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 1..3,
            })
        );
        assert_eq!(
            eval("0>>0x100000000", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 1..3,
            })
        );
    }

    #[test]
    fn bit_shifting_of_negative_values() {
        let env = &mut HashMap::new();

        // Left-shifting a negative value is undefined in C.
        assert_eq!(
            eval("-1<<1", env),
            Err(Error {
                cause: ErrorCause::LeftShiftingNegative,
                location: 2..4,
            })
        );
        assert_eq!(
            eval("(-0x7FFFFFFFFFFFFFFF-1)<<1", env),
            Err(Error {
                cause: ErrorCause::LeftShiftingNegative,
                location: 23..25,
            })
        );

        // Right-shifting a negative value is implementation-defined in C.
        assert_eq!(eval("-4>>1", env), Ok(Value::Integer(-4 >> 1)));
        assert_eq!(eval("-1>>1", env), Ok(Value::Integer(-1 >> 1)));
    }

    #[test]
    fn reverse_bit_shifting() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("1 << -1", env),
            Err(Error {
                cause: ErrorCause::ReverseShifting,
                location: 2..4,
            })
        );

        assert_eq!(
            eval("1 >> -1", env),
            Err(Error {
                cause: ErrorCause::ReverseShifting,
                location: 2..4,
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

    #[test]
    fn overflow_in_division() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("(-0x7FFFFFFFFFFFFFFF-1)/-1", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 23..24,
            })
        );
    }

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

    #[test]
    fn overflow_in_remainder() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("(-0x7FFFFFFFFFFFFFFF-1)%-1", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 23..24,
            })
        );
    }

    #[test]
    fn plus_prefix_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("+0", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" + 10 ", env), Ok(Value::Integer(10)));
        assert_eq!(eval(" + + 57", env), Ok(Value::Integer(57)));
    }

    #[test]
    fn numeric_negation_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("-0", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" - 12 ", env), Ok(Value::Integer(-12)));
        assert_eq!(eval(" - - 49", env), Ok(Value::Integer(49)));
        assert_eq!(eval(" - - - 49", env), Ok(Value::Integer(-49)));
    }

    #[test]
    fn overflow_in_numeric_negation() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval("-0x7FFFFFFFFFFFFFFF-1", env),
            Ok(Value::Integer(i64::MIN))
        );
        assert_eq!(
            eval(" - (-0x7FFFFFFFFFFFFFFF-1)", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 1..2
            })
        );
    }

    #[test]
    fn bitwise_negation_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("~0", env), Ok(Value::Integer(-1)));
        assert_eq!(eval(" ~ 3 ", env), Ok(Value::Integer(!3)));
        assert_eq!(eval(" ~ ~ 42", env), Ok(Value::Integer(42)));
        assert_eq!(eval(" ~ ~ ~ 0x38E7", env), Ok(Value::Integer(!0x38E7)));
    }

    #[test]
    fn logical_negation_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("!0", env), Ok(Value::Integer(1)));
        assert_eq!(eval(" ! 1 ", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" ! 2 ", env), Ok(Value::Integer(0)));
        assert_eq!(eval(" ! ! 3", env), Ok(Value::Integer(1)));
    }

    #[test]
    fn prefix_increment_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("++a", env), Ok(Value::Integer(1)));
        assert_eq!(eval("++a", env), Ok(Value::Integer(2)));
        assert_eq!(eval("++a", env), Ok(Value::Integer(3)));
        assert_eq!(eval("a", env), Ok(Value::Integer(3)));
    }

    // TODO prefix_incrementing_non_variable eval("++ +a")

    #[test]
    fn overflow_in_increment() {
        let env = &mut HashMap::new();
        env.assign_variable("i", "9223372036854775807".to_string())
            .unwrap();
        assert_eq!(
            eval("  ++ i", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 2..4,
            })
        );
    }

    #[test]
    fn prefix_decrement_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("--d", env), Ok(Value::Integer(-1)));
        assert_eq!(eval("--d", env), Ok(Value::Integer(-2)));
        assert_eq!(eval("--d", env), Ok(Value::Integer(-3)));
        assert_eq!(eval("d", env), Ok(Value::Integer(-3)));
    }

    #[test]
    fn overflow_in_decrement() {
        let env = &mut HashMap::new();
        env.assign_variable("i", "-9223372036854775808".to_string())
            .unwrap();
        assert_eq!(
            eval(" -- i", env),
            Err(Error {
                cause: ErrorCause::Overflow,
                location: 1..3,
            })
        );
    }

    // TODO prefix_decrementing_non_variable eval("-- +a")

    #[test]
    fn postfix_increment_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("a++", env), Ok(Value::Integer(0)));
        assert_eq!(eval("a++", env), Ok(Value::Integer(1)));
        assert_eq!(eval("a++", env), Ok(Value::Integer(2)));
        assert_eq!(eval("a", env), Ok(Value::Integer(3)));
    }

    // TODO postfix_incrementing_non_variable eval("5++")

    #[test]
    fn postfix_decrement_operator() {
        let env = &mut HashMap::new();
        assert_eq!(eval("a--", env), Ok(Value::Integer(0)));
        assert_eq!(eval("a--", env), Ok(Value::Integer(-1)));
        assert_eq!(eval("a--", env), Ok(Value::Integer(-2)));
        assert_eq!(eval("a", env), Ok(Value::Integer(-3)));
    }

    // TODO postfix_decrementing_non_variable eval("7--")

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

    #[test]
    fn combining_prefix_and_postfix_operators() {
        let env = &mut HashMap::new();
        assert_eq!(eval("+a++", env), Ok(Value::Integer(0)));
        assert_eq!(eval("-a++", env), Ok(Value::Integer(-1)));
        assert_eq!(eval("~a--", env), Ok(Value::Integer(-3)));
        assert_eq!(eval("!a--", env), Ok(Value::Integer(0)));
    }

    #[test]
    fn parentheses() {
        let env = &mut HashMap::new();
        assert_eq!(eval("(42)", env), Ok(Value::Integer(42)));
        assert_eq!(eval("(1+2)", env), Ok(Value::Integer(3)));
        assert_eq!(eval("(2+3)*4", env), Ok(Value::Integer(20)));
        assert_eq!(eval("2*(3+4)", env), Ok(Value::Integer(14)));
        assert_eq!(eval(" ( 6 - ( 7 - 3 ) ) * 2 ", env), Ok(Value::Integer(4)));
        assert_eq!(eval(" 4 | ( ( 2 && 2 ) & 3 )", env), Ok(Value::Integer(5)));
    }

    #[test]
    fn combining_postfix_operator_and_parentheses() {
        let env = &mut HashMap::new();
        assert_eq!(eval("(a)++", env), Ok(Value::Integer(0)));
        assert_eq!(eval("(a) --", env), Ok(Value::Integer(1)));
        assert_eq!(eval("a", env), Ok(Value::Integer(0)));
    }

    // TODO unmatched_parentheses
}
