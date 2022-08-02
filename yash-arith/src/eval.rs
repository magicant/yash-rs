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

//! Evaluation of the expression

use crate::ast::Ast;
use crate::ast::PrefixOperator;
use crate::env::Env;
use crate::token::Term;
use crate::token::Value;
use std::ops::Range;

/// Cause of an evaluation error
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum EvalError<E> {
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

/// Description of an error that occurred during evaluation
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Error<E> {
    /// Cause of the error
    pub cause: EvalError<E>,
    /// Range of the substring in the evaluated expression string where the error occurred
    pub location: Range<usize>,
}

/// Expands a variable to its value.
fn expand_variable<E: Env>(
    name: &str,
    location: &Range<usize>,
    env: &E,
) -> Result<Value, Error<E::AssignVariableError>> {
    if let Some(value) = env.get_variable(name) {
        // TODO Parse non-decimal integer and float
        match value.parse() {
            Ok(number) => Ok(Value::Integer(number)),
            Err(_) => Err(Error {
                cause: EvalError::InvalidVariableValue(value.to_string()),
                location: location.clone(),
            }),
        }
    } else {
        Ok(Value::Integer(0))
    }
}

/// Evaluates a term into a value.
fn into_value<E: Env>(term: Term, env: &E) -> Result<Value, Error<E::AssignVariableError>> {
    match term {
        Term::Value(value) => Ok(value),
        Term::Variable { name, location } => expand_variable(name, &location, env),
    }
}

/// Extracts a successful computation result or returns an overflow error.
fn unwrap_or_overflow<T, E>(
    checked_computation: Option<T>,
    location: &Range<usize>,
) -> Result<T, Error<E>> {
    checked_computation.ok_or_else(|| Error {
        cause: EvalError::Overflow,
        location: location.clone(),
    })
}

/// Assigns a value to a variable and returns the value.
fn assign<E: Env>(
    name: &str,
    value: Value,
    location: Range<usize>,
    env: &mut E,
) -> Result<Value, Error<E::AssignVariableError>> {
    match env.assign_variable(name, value.to_string()) {
        Ok(()) => Ok(value),
        Err(e) => Err(Error {
            cause: EvalError::AssignVariableError(e),
            location,
        }),
    }
}

/// Applies a prefix operator to a term.
fn apply_prefix<'a, E: Env>(
    term: Term<'a>,
    operator: PrefixOperator,
    op_location: &Range<usize>,
    env: &mut E,
) -> Result<Value, Error<E::AssignVariableError>> {
    match operator {
        PrefixOperator::Increment => match term {
            Term::Value(_) => todo!("reject non-variable"),
            Term::Variable { name, location } => match expand_variable(name, &location, env)? {
                Value::Integer(value) => {
                    let new_value =
                        Value::Integer(unwrap_or_overflow(value.checked_add(1), op_location)?);
                    assign(name, new_value, location, env)
                }
            },
        },
        PrefixOperator::Decrement => match term {
            Term::Value(_) => todo!("reject non-variable"),
            Term::Variable { name, location } => match expand_variable(name, &location, env)? {
                Value::Integer(value) => {
                    let new_value =
                        Value::Integer(unwrap_or_overflow(value.checked_sub(1), op_location)?);
                    assign(name, new_value, location, env)
                }
            },
        },
        PrefixOperator::NumericCoercion => into_value(term, env),
        PrefixOperator::NumericNegation => match into_value(term, env)? {
            Value::Integer(value) => match value.checked_neg() {
                Some(result) => Ok(Value::Integer(result)),
                None => Err(Error {
                    cause: EvalError::Overflow,
                    location: op_location.clone(),
                }),
            },
        },
        PrefixOperator::LogicalNegation => match into_value(term, env)? {
            Value::Integer(value) => Ok(Value::Integer((value == 0) as _)),
        },
        PrefixOperator::BitwiseNegation => match into_value(term, env)? {
            Value::Integer(value) => Ok(Value::Integer(!value)),
        },
    }
}

/// Evaluates an expression.
///
/// The given `ast` must not be empty, or this function will **panic**.
pub fn eval<'a, E: Env>(
    ast: &[Ast<'a>],
    env: &mut E,
) -> Result<Term<'a>, Error<E::AssignVariableError>> {
    let (root, children) = ast.split_last().expect("evaluating an empty expression");
    match root {
        Ast::Term(term) => Ok(term.clone()),

        Ast::Prefix { operator, location } => {
            let term = eval(children, env)?;
            apply_prefix(term, *operator, location, env).map(Term::Value)
        }

        Ast::Postfix { operator, location } => todo!(),
        Ast::Binary {
            operator,
            rhs_len,
            location,
        } => todo!(),
        Ast::Conditional { then_len, else_len } => todo!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn expand_variable_non_existing() {
        let env = &mut HashMap::new();
        assert_eq!(expand_variable("a", &(10..11), env), Ok(Value::Integer(0)));
        assert_eq!(expand_variable("b", &(11..12), env), Ok(Value::Integer(0)));
    }

    #[test]
    fn expand_variable_valid() {
        let env = &mut HashMap::new();
        env.insert("a".to_string(), "42".to_string());
        env.insert("b".to_string(), "-123".to_string());
        assert_eq!(expand_variable("a", &(10..11), env), Ok(Value::Integer(42)));
        assert_eq!(
            expand_variable("b", &(11..12), env),
            Ok(Value::Integer(-123))
        );
    }

    #[test]
    fn expand_variable_invalid() {
        let env = &mut HashMap::new();
        env.insert("a".to_string(), "*".to_string());
        assert_eq!(
            expand_variable("a", &(10..11), env),
            Err(Error {
                cause: EvalError::InvalidVariableValue("*".to_string()),
                location: 10..11,
            })
        );
    }

    #[test]
    fn apply_prefix_increment() {
        let env = &mut HashMap::new();

        assert_eq!(
            apply_prefix(
                Term::Variable {
                    name: "i",
                    location: 6..7
                },
                PrefixOperator::Increment,
                &(3..5),
                env
            ),
            Ok(Value::Integer(1))
        );
        assert_eq!(env["i"], "1");

        assert_eq!(
            apply_prefix(
                Term::Variable {
                    name: "i",
                    location: 6..7
                },
                PrefixOperator::Increment,
                &(3..5),
                env
            ),
            Ok(Value::Integer(2))
        );
        assert_eq!(env["i"], "2");
    }

    #[test]
    fn apply_prefix_increment_overflow() {
        let env = &mut HashMap::new();
        env.insert("i".to_string(), "9223372036854775807".to_string());
        assert_eq!(
            apply_prefix(
                Term::Variable {
                    name: "i",
                    location: 6..7
                },
                PrefixOperator::Increment,
                &(3..5),
                env
            ),
            Err(Error {
                cause: EvalError::Overflow,
                location: 3..5,
            })
        );
    }

    // TODO apply_prefix_increment_not_variable

    #[test]
    fn apply_prefix_decrement() {
        let env = &mut HashMap::new();

        assert_eq!(
            apply_prefix(
                Term::Variable {
                    name: "i",
                    location: 6..7
                },
                PrefixOperator::Decrement,
                &(3..5),
                env
            ),
            Ok(Value::Integer(-1))
        );
        assert_eq!(env["i"], "-1");

        assert_eq!(
            apply_prefix(
                Term::Variable {
                    name: "i",
                    location: 6..7
                },
                PrefixOperator::Decrement,
                &(3..5),
                env
            ),
            Ok(Value::Integer(-2))
        );
        assert_eq!(env["i"], "-2");
    }

    #[test]
    fn apply_prefix_decrement_overflow() {
        let env = &mut HashMap::new();
        env.insert("i".to_string(), "-9223372036854775808".to_string());
        assert_eq!(
            apply_prefix(
                Term::Variable {
                    name: "i",
                    location: 6..7
                },
                PrefixOperator::Decrement,
                &(3..5),
                env
            ),
            Err(Error {
                cause: EvalError::Overflow,
                location: 3..5,
            })
        );
    }

    // TODO apply_prefix_decrement_not_variable

    #[test]
    fn apply_prefix_numeric_coercion() {
        let env = &mut HashMap::new();
        env.insert("a".to_string(), "12".to_string());
        assert_eq!(
            apply_prefix(
                Term::Value(Value::Integer(7)),
                PrefixOperator::NumericCoercion,
                &(3..4),
                env
            ),
            Ok(Value::Integer(7))
        );
        assert_eq!(
            apply_prefix(
                Term::Variable {
                    name: "a",
                    location: 5..7,
                },
                PrefixOperator::NumericCoercion,
                &(3..4),
                env
            ),
            Ok(Value::Integer(12))
        );
    }

    #[test]
    fn apply_prefix_numeric_negation() {
        let env = &mut HashMap::new();
        assert_eq!(
            apply_prefix(
                Term::Value(Value::Integer(7)),
                PrefixOperator::NumericNegation,
                &(3..4),
                env
            ),
            Ok(Value::Integer(-7))
        );
        assert_eq!(
            apply_prefix(
                Term::Value(Value::Integer(-10)),
                PrefixOperator::NumericNegation,
                &(3..4),
                env
            ),
            Ok(Value::Integer(10))
        );
    }

    #[test]
    fn apply_prefix_numeric_negation_overflow() {
        let env = &mut HashMap::new();
        assert_eq!(
            apply_prefix(
                Term::Value(Value::Integer(i64::MIN)),
                PrefixOperator::NumericNegation,
                &(3..4),
                env
            ),
            Err(Error {
                cause: EvalError::Overflow,
                location: 3..4,
            })
        );
    }

    #[test]
    fn apply_prefix_logical_negation() {
        let env = &mut HashMap::new();
        assert_eq!(
            apply_prefix(
                Term::Value(Value::Integer(0)),
                PrefixOperator::LogicalNegation,
                &(3..4),
                env
            ),
            Ok(Value::Integer(1))
        );

        for i in [-1, 1, 2, 100, i64::MAX, i64::MIN] {
            assert_eq!(
                apply_prefix(
                    Term::Value(Value::Integer(i)),
                    PrefixOperator::LogicalNegation,
                    &(3..4),
                    env
                ),
                Ok(Value::Integer(0)),
                "i={:?}",
                i
            );
        }
    }

    #[test]
    fn apply_prefix_bitwise_negation() {
        let env = &mut HashMap::new();
        assert_eq!(
            apply_prefix(
                Term::Value(Value::Integer(0)),
                PrefixOperator::BitwiseNegation,
                &(3..4),
                env
            ),
            Ok(Value::Integer(!0))
        );
        assert_eq!(
            apply_prefix(
                Term::Value(Value::Integer(-10000)),
                PrefixOperator::BitwiseNegation,
                &(3..4),
                env
            ),
            Ok(Value::Integer(!-10000))
        );
    }

    #[test]
    fn eval_term() {
        let env = &mut HashMap::new();

        let t = Term::Value(Value::Integer(42));
        assert_eq!(eval(&[Ast::Term(t.clone())], env), Ok(t));

        let t = Term::Variable {
            name: "a",
            location: 10..11,
        };
        assert_eq!(eval(&[Ast::Term(t.clone())], env), Ok(t));
    }

    #[test]
    fn eval_prefix() {
        let env = &mut HashMap::new();
        let ast = &[
            Ast::Term(Term::Value(Value::Integer(15))),
            Ast::Prefix {
                operator: PrefixOperator::NumericNegation,
                location: 2..3,
            },
        ];
        assert_eq!(eval(ast, env), Ok(Term::Value(Value::Integer(-15))));
    }
}
