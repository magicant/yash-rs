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
use crate::ast::BinaryOperator;
use crate::ast::PostfixOperator;
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
    /// Assignment with a left-hand-side operand not being a variable
    AssignmentToValue,
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

/// Tests if a term is a variable.
///
/// If the term is a value, returns an `AssignmentToValue` error with the given
/// location.
fn require_variable<'a, E>(
    term: Term<'a>,
    op_location: &Range<usize>,
) -> Result<(&'a str, Range<usize>), Error<E>> {
    match term {
        Term::Variable { name, location } => Ok((name, location)),
        Term::Value(_) => Err(Error {
            cause: EvalError::AssignmentToValue,
            location: op_location.clone(),
        }),
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
        PrefixOperator::Increment => {
            let (name, location) = require_variable(term, op_location)?;
            match expand_variable(name, &location, env)? {
                Value::Integer(value) => {
                    let new_value =
                        Value::Integer(unwrap_or_overflow(value.checked_add(1), op_location)?);
                    assign(name, new_value, location, env)
                }
            }
        }
        PrefixOperator::Decrement => {
            let (name, location) = require_variable(term, op_location)?;
            match expand_variable(name, &location, env)? {
                Value::Integer(value) => {
                    let new_value =
                        Value::Integer(unwrap_or_overflow(value.checked_sub(1), op_location)?);
                    assign(name, new_value, location, env)
                }
            }
        }
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

/// Applies a postfix operator to a term.
fn apply_postfix<'a, E: Env>(
    term: Term<'a>,
    operator: PostfixOperator,
    op_location: &Range<usize>,
    env: &mut E,
) -> Result<Value, Error<E::AssignVariableError>> {
    let (name, location) = require_variable(term, op_location)?;
    match expand_variable(name, &location, env)? {
        old_value @ Value::Integer(value) => {
            let result = match operator {
                PostfixOperator::Increment => value.checked_add(1),
                PostfixOperator::Decrement => value.checked_sub(1),
            };
            let new_value = Value::Integer(unwrap_or_overflow(result, op_location)?);
            assign(name, new_value, location, env)?;
            Ok(old_value)
        }
    }
}

/// Computes the result value of a binary operator.
///
/// If `operator` is a compound assignment operator, this function only computes
/// the result value without performing assignment.
fn binary_result<E>(
    lhs: Value,
    rhs: Value,
    operator: BinaryOperator,
    op_location: &Range<usize>,
) -> Result<Value, Error<E>> {
    fn require_non_negative<E>(v: i64, location: &Range<usize>) -> Result<u32, Error<E>> {
        v.try_into().map_err(|_| Error {
            cause: EvalError::ReverseShifting,
            location: location.clone(),
        })
    }
    fn require_non_zero<E>(v: i64, location: &Range<usize>) -> Result<(), Error<E>> {
        if v != 0 {
            Ok(())
        } else {
            Err(Error {
                cause: EvalError::DivisionByZero,
                location: location.clone(),
            })
        }
    }

    let Value::Integer(lhs) = lhs;
    let Value::Integer(rhs) = rhs;
    use BinaryOperator::*;
    let result = match operator {
        LogicalOr => Some((lhs != 0 || rhs != 0) as _),
        LogicalAnd => Some((lhs != 0 && rhs != 0) as _),
        BitwiseOr | BitwiseOrAssign => Some(lhs | rhs),
        BitwiseXor | BitwiseXorAssign => Some(lhs ^ rhs),
        BitwiseAnd | BitwiseAndAssign => Some(lhs & rhs),
        EqualTo => Some((lhs == rhs) as _),
        NotEqualTo => Some((lhs != rhs) as _),
        LessThan => Some((lhs < rhs) as _),
        GreaterThan => Some((lhs > rhs) as _),
        LessThanOrEqualTo => Some((lhs <= rhs) as _),
        GreaterThanOrEqualTo => Some((lhs >= rhs) as _),
        ShiftLeft | ShiftLeftAssign => {
            if lhs < 0 {
                return Err(Error {
                    cause: EvalError::LeftShiftingNegative,
                    location: op_location.clone(),
                });
            }
            let rhs = require_non_negative(rhs, op_location)?;
            lhs.checked_shl(rhs)
                .filter(|&result| result >= 0 && result >> rhs == lhs)
        }
        ShiftRight | ShiftRightAssign => {
            let rhs = require_non_negative(rhs, op_location)?;
            lhs.checked_shr(rhs)
        }
        Add | AddAssign => lhs.checked_add(rhs),
        Subtract | SubtractAssign => lhs.checked_sub(rhs),
        Multiply | MultiplyAssign => lhs.checked_mul(rhs),
        Divide | DivideAssign => {
            require_non_zero(rhs, op_location)?;
            lhs.checked_div(rhs)
        }
        Remainder | RemainderAssign => {
            require_non_zero(rhs, op_location)?;
            lhs.checked_rem(rhs)
        }
        Assign => Some(rhs),
    };
    let result = unwrap_or_overflow(result, op_location)?;
    Ok(Value::Integer(result))
}

/// Applies a binary operator.
fn apply_binary<'a, E: Env>(
    lhs: Term<'a>,
    rhs: Term<'a>,
    operator: BinaryOperator,
    op_location: &Range<usize>,
    env: &mut E,
) -> Result<Value, Error<E::AssignVariableError>> {
    use BinaryOperator::*;
    match operator {
        LogicalOr | LogicalAnd | BitwiseOr | BitwiseXor | BitwiseAnd | EqualTo | NotEqualTo
        | LessThan | GreaterThan | LessThanOrEqualTo | GreaterThanOrEqualTo | ShiftLeft
        | ShiftRight | Add | Subtract | Multiply | Divide | Remainder => {
            let lhs = into_value(lhs, env)?;
            let rhs = into_value(rhs, env)?;
            binary_result(lhs, rhs, operator, op_location)
        }
        Assign => {
            let (name, location) = require_variable(lhs, op_location)?;
            let value = into_value(rhs, env)?;
            assign(name, value, location, env)
        }
        BitwiseOrAssign | BitwiseXorAssign | BitwiseAndAssign | ShiftLeftAssign
        | ShiftRightAssign | AddAssign | SubtractAssign | MultiplyAssign | DivideAssign
        | RemainderAssign => {
            let (name, location) = require_variable(lhs, op_location)?;
            let lhs = expand_variable(name, &location, env)?;
            let rhs = into_value(rhs, env)?;
            let result = binary_result(lhs, rhs, operator, op_location)?;
            assign(name, result, location, env)
        }
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

        Ast::Postfix { operator, location } => {
            let term = eval(children, env)?;
            apply_postfix(term, *operator, location, env).map(Term::Value)
        }

        Ast::Binary {
            operator: BinaryOperator::LogicalOr,
            rhs_len,
            location,
        } => {
            let (lhs_ast, rhs_ast) = children.split_at(children.len() - rhs_len);
            let lhs = into_value(eval(lhs_ast, env)?, env)?;
            if lhs != Value::Integer(0) {
                return Ok(Term::Value(Value::Integer(1)));
            }
            let rhs = into_value(eval(rhs_ast, env)?, env)?;
            binary_result(lhs, rhs, BinaryOperator::LogicalOr, location).map(Term::Value)
        }

        Ast::Binary {
            operator: BinaryOperator::LogicalAnd,
            rhs_len,
            location,
        } => {
            let (lhs_ast, rhs_ast) = children.split_at(children.len() - rhs_len);
            let lhs = into_value(eval(lhs_ast, env)?, env)?;
            if lhs == Value::Integer(0) {
                return Ok(Term::Value(Value::Integer(0)));
            }
            let rhs = into_value(eval(rhs_ast, env)?, env)?;
            binary_result(lhs, rhs, BinaryOperator::LogicalAnd, location).map(Term::Value)
        }

        Ast::Binary {
            operator,
            rhs_len,
            location,
        } => {
            let (lhs_ast, rhs_ast) = children.split_at(children.len() - rhs_len);
            let lhs = eval(lhs_ast, env)?;
            let rhs = eval(rhs_ast, env)?;
            apply_binary(lhs, rhs, *operator, location, env).map(Term::Value)
        }

        Ast::Conditional { then_len, else_len } => {
            let (children_2, else_ast) = children.split_at(children.len() - else_len);
            let (condition_ast, then_ast) = children_2.split_at(children_2.len() - then_len);
            let condition = into_value(eval(condition_ast, env)?, env)?;
            let result_ast = if condition != Value::Integer(0) {
                then_ast
            } else {
                else_ast
            };
            eval(result_ast, env)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::convert::Infallible;

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

    #[test]
    fn apply_prefix_increment_not_variable() {
        let env = &mut HashMap::new();
        assert_eq!(
            apply_prefix(
                Term::Value(Value::Integer(3)),
                PrefixOperator::Increment,
                &(3..5),
                env
            ),
            Err(Error {
                cause: EvalError::AssignmentToValue,
                location: 3..5,
            })
        );
    }

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

    #[test]
    fn apply_prefix_decrement_not_variable() {
        let env = &mut HashMap::new();
        assert_eq!(
            apply_prefix(
                Term::Value(Value::Integer(3)),
                PrefixOperator::Decrement,
                &(3..5),
                env
            ),
            Err(Error {
                cause: EvalError::AssignmentToValue,
                location: 3..5,
            })
        );
    }

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
    fn apply_postfix_increment() {
        let env = &mut HashMap::new();

        assert_eq!(
            apply_postfix(
                Term::Variable {
                    name: "i",
                    location: 0..1,
                },
                PostfixOperator::Increment,
                &(3..5),
                env
            ),
            Ok(Value::Integer(0))
        );
        assert_eq!(env["i"], "1");

        assert_eq!(
            apply_postfix(
                Term::Variable {
                    name: "i",
                    location: 0..1,
                },
                PostfixOperator::Increment,
                &(3..5),
                env
            ),
            Ok(Value::Integer(1))
        );
        assert_eq!(env["i"], "2");
    }

    #[test]
    fn apply_postfix_increment_overflow() {
        let env = &mut HashMap::new();
        env.insert("i".to_string(), "9223372036854775807".to_string());
        assert_eq!(
            apply_postfix(
                Term::Variable {
                    name: "i",
                    location: 0..1,
                },
                PostfixOperator::Increment,
                &(3..5),
                env
            ),
            Err(Error {
                cause: EvalError::Overflow,
                location: 3..5,
            })
        );
    }

    #[test]
    fn apply_postfix_increment_not_variable() {
        let env = &mut HashMap::new();
        assert_eq!(
            apply_postfix(
                Term::Value(Value::Integer(13)),
                PostfixOperator::Increment,
                &(3..5),
                env
            ),
            Err(Error {
                cause: EvalError::AssignmentToValue,
                location: 3..5,
            })
        );
    }

    #[test]
    fn apply_postfix_decrement() {
        let env = &mut HashMap::new();

        assert_eq!(
            apply_postfix(
                Term::Variable {
                    name: "i",
                    location: 0..1,
                },
                PostfixOperator::Decrement,
                &(3..5),
                env
            ),
            Ok(Value::Integer(0))
        );
        assert_eq!(env["i"], "-1");

        assert_eq!(
            apply_postfix(
                Term::Variable {
                    name: "i",
                    location: 0..1,
                },
                PostfixOperator::Decrement,
                &(3..5),
                env
            ),
            Ok(Value::Integer(-1))
        );
        assert_eq!(env["i"], "-2");
    }

    #[test]
    fn apply_postfix_decrement_overflow() {
        let env = &mut HashMap::new();
        env.insert("i".to_string(), "-9223372036854775808".to_string());
        assert_eq!(
            apply_postfix(
                Term::Variable {
                    name: "i",
                    location: 0..1,
                },
                PostfixOperator::Decrement,
                &(3..5),
                env
            ),
            Err(Error {
                cause: EvalError::Overflow,
                location: 3..5,
            })
        );
    }

    #[test]
    fn apply_postfix_decrement_not_variable() {
        let env = &mut HashMap::new();
        assert_eq!(
            apply_postfix(
                Term::Value(Value::Integer(13)),
                PostfixOperator::Decrement,
                &(3..5),
                env
            ),
            Err(Error {
                cause: EvalError::AssignmentToValue,
                location: 3..5,
            })
        );
    }

    #[test]
    fn binary_result_logical_or() {
        let zero = Value::Integer(0);
        let one = Value::Integer(1);
        let two = Value::Integer(2);
        let operator = BinaryOperator::LogicalOr;
        let result = binary_result::<Infallible>(zero, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(one, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(zero, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
    }

    #[test]
    fn binary_result_logical_and() {
        let zero = Value::Integer(0);
        let one = Value::Integer(1);
        let two = Value::Integer(2);
        let operator = BinaryOperator::LogicalAnd;
        let result = binary_result::<Infallible>(zero, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(zero, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
    }

    #[test]
    fn binary_result_bitwise_or() {
        let zero = Value::Integer(0);
        let three = Value::Integer(3);
        let six = Value::Integer(6);
        for operator in [BinaryOperator::BitwiseOr, BinaryOperator::BitwiseOrAssign] {
            let result = binary_result::<Infallible>(zero, zero, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(0)));
            let result = binary_result::<Infallible>(three, zero, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(3)));
            let result = binary_result::<Infallible>(three, six, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(7)));
            let result = binary_result::<Infallible>(zero, six, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(6)));
        }
    }

    #[test]
    fn binary_result_bitwise_xor() {
        let zero = Value::Integer(0);
        let three = Value::Integer(3);
        let six = Value::Integer(6);
        for operator in [BinaryOperator::BitwiseXor, BinaryOperator::BitwiseXorAssign] {
            let result = binary_result::<Infallible>(zero, zero, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(0)));
            let result = binary_result::<Infallible>(three, zero, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(3)));
            let result = binary_result::<Infallible>(three, six, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(5)));
            let result = binary_result::<Infallible>(zero, six, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(6)));
        }
    }

    #[test]
    fn binary_result_bitwise_and() {
        let zero = Value::Integer(0);
        let three = Value::Integer(3);
        let six = Value::Integer(6);
        for operator in [BinaryOperator::BitwiseAnd, BinaryOperator::BitwiseAndAssign] {
            let result = binary_result::<Infallible>(zero, zero, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(0)));
            let result = binary_result::<Infallible>(three, zero, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(0)));
            let result = binary_result::<Infallible>(three, six, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(2)));
            let result = binary_result::<Infallible>(zero, six, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(0)));
        }
    }

    #[test]
    fn binary_result_equal_to() {
        let zero = Value::Integer(0);
        let one = Value::Integer(1);
        let two = Value::Integer(2);
        let operator = BinaryOperator::EqualTo;
        let result = binary_result::<Infallible>(zero, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(one, one, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(two, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(one, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(zero, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
    }

    #[test]
    fn binary_result_not_equal_to() {
        let zero = Value::Integer(0);
        let one = Value::Integer(1);
        let two = Value::Integer(2);
        let operator = BinaryOperator::NotEqualTo;
        let result = binary_result::<Infallible>(zero, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, one, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(two, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(one, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(zero, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
    }

    #[test]
    fn binary_result_less_than() {
        let zero = Value::Integer(0);
        let one = Value::Integer(1);
        let two = Value::Integer(2);
        let operator = BinaryOperator::LessThan;
        let result = binary_result::<Infallible>(zero, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, one, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(two, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(zero, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
    }

    #[test]
    fn binary_result_greater_than() {
        let zero = Value::Integer(0);
        let one = Value::Integer(1);
        let two = Value::Integer(2);
        let operator = BinaryOperator::GreaterThan;
        let result = binary_result::<Infallible>(zero, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, one, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(two, one, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(one, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(one, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(zero, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
    }

    #[test]
    fn binary_result_less_than_or_equal_to() {
        let zero = Value::Integer(0);
        let one = Value::Integer(1);
        let two = Value::Integer(2);
        let operator = BinaryOperator::LessThanOrEqualTo;
        let result = binary_result::<Infallible>(zero, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(one, one, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(two, one, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(one, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(zero, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
    }

    #[test]
    fn binary_result_greater_than_or_equal_to() {
        let zero = Value::Integer(0);
        let one = Value::Integer(1);
        let two = Value::Integer(2);
        let operator = BinaryOperator::GreaterThanOrEqualTo;
        let result = binary_result::<Infallible>(zero, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(one, one, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(two, one, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(one, zero, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(1)));
        let result = binary_result::<Infallible>(one, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
        let result = binary_result::<Infallible>(zero, two, operator, &(3..5));
        assert_eq!(result, Ok(Value::Integer(0)));
    }

    #[test]
    fn binary_result_shift_left() {
        let lhs = Value::Integer(0x94E239);
        let rhs = Value::Integer(7);
        for operator in [BinaryOperator::ShiftLeft, BinaryOperator::ShiftLeftAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(0x94E239 << 7)));
        }
    }

    #[test]
    fn binary_result_shift_left_negative_lhs() {
        let lhs = Value::Integer(-1);
        let rhs = Value::Integer(0);
        for operator in [BinaryOperator::ShiftLeft, BinaryOperator::ShiftLeftAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::LeftShiftingNegative,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_shift_left_negative_rhs() {
        let lhs = Value::Integer(0);
        let rhs = Value::Integer(-1);
        for operator in [BinaryOperator::ShiftLeft, BinaryOperator::ShiftLeftAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::ReverseShifting,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_shift_left_too_large_rhs() {
        let lhs = Value::Integer(0);
        let rhs = Value::Integer(i64::BITS as _);
        for operator in [BinaryOperator::ShiftLeft, BinaryOperator::ShiftLeftAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::Overflow,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_shift_left_overflow_to_sign_bit() {
        let lhs = Value::Integer(0x4000_0000_0000_0000);
        let rhs = Value::Integer(1);
        for operator in [BinaryOperator::ShiftLeft, BinaryOperator::ShiftLeftAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::Overflow,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_shift_left_overflow_beyond_sign_bit() {
        let lhs = Value::Integer(0x4000_0000_0000_0000);
        let rhs = Value::Integer(2);
        for operator in [BinaryOperator::ShiftLeft, BinaryOperator::ShiftLeftAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::Overflow,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_shift_right() {
        let lhs = Value::Integer(0x94E239);
        let rhs = Value::Integer(7);
        for operator in [BinaryOperator::ShiftRight, BinaryOperator::ShiftRightAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(0x94E239 >> 7)));
        }
    }

    #[test]
    fn binary_result_shift_right_negative_rhs() {
        let lhs = Value::Integer(0);
        let rhs = Value::Integer(-1);
        for operator in [BinaryOperator::ShiftRight, BinaryOperator::ShiftRightAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::ReverseShifting,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_shift_right_too_large_rhs() {
        let lhs = Value::Integer(0);
        let rhs = Value::Integer(i64::BITS as _);
        for operator in [BinaryOperator::ShiftRight, BinaryOperator::ShiftRightAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::Overflow,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_add() {
        let lhs = Value::Integer(15);
        let rhs = Value::Integer(27);
        for operator in [BinaryOperator::Add, BinaryOperator::AddAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(42)));
        }
    }

    #[test]
    fn binary_result_add_overflow() {
        let lhs = Value::Integer(i64::MIN);
        let rhs = Value::Integer(-1);
        for operator in [BinaryOperator::Add, BinaryOperator::AddAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::Overflow,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_subtract() {
        let lhs = Value::Integer(15);
        let rhs = Value::Integer(27);
        for operator in [BinaryOperator::Subtract, BinaryOperator::SubtractAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(-12)));
        }
    }

    #[test]
    fn binary_result_subtract_overflow() {
        let lhs = Value::Integer(i64::MAX);
        let rhs = Value::Integer(-1);
        for operator in [BinaryOperator::Subtract, BinaryOperator::SubtractAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::Overflow,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_multiply() {
        let lhs = Value::Integer(15);
        let rhs = Value::Integer(27);
        for operator in [BinaryOperator::Multiply, BinaryOperator::MultiplyAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(405)));
        }
    }

    #[test]
    fn binary_result_multiply_overflow() {
        let lhs = Value::Integer(0x4000_0000_0000_0000);
        let rhs = Value::Integer(2);
        for operator in [BinaryOperator::Multiply, BinaryOperator::MultiplyAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::Overflow,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_divide() {
        let lhs = Value::Integer(268);
        let rhs = Value::Integer(17);
        for operator in [BinaryOperator::Divide, BinaryOperator::DivideAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(15)));
        }
    }

    #[test]
    fn binary_result_divide_overflow() {
        let lhs = Value::Integer(i64::MIN);
        let rhs = Value::Integer(-1);
        for operator in [BinaryOperator::Divide, BinaryOperator::DivideAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::Overflow,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_divide_by_zero() {
        let lhs = Value::Integer(1);
        let rhs = Value::Integer(0);
        for operator in [BinaryOperator::Divide, BinaryOperator::DivideAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::DivisionByZero,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_remainder() {
        let lhs = Value::Integer(268);
        let rhs = Value::Integer(17);
        for operator in [BinaryOperator::Remainder, BinaryOperator::RemainderAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(result, Ok(Value::Integer(13)));
        }
    }

    #[test]
    fn binary_result_remainder_overflow() {
        let lhs = Value::Integer(i64::MIN);
        let rhs = Value::Integer(-1);
        for operator in [BinaryOperator::Remainder, BinaryOperator::RemainderAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::Overflow,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn binary_result_remainder_by_zero() {
        let lhs = Value::Integer(1);
        let rhs = Value::Integer(0);
        for operator in [BinaryOperator::Remainder, BinaryOperator::RemainderAssign] {
            let result = binary_result::<Infallible>(lhs, rhs, operator, &(3..4));
            assert_eq!(
                result,
                Err(Error {
                    cause: EvalError::DivisionByZero,
                    location: 3..4,
                })
            );
        }
    }

    #[test]
    fn apply_binary_add() {
        let env = &mut HashMap::new();
        let lhs = Term::Value(Value::Integer(30));
        let rhs = Term::Value(Value::Integer(12));
        let operator = BinaryOperator::Add;
        let op_location = 4..5;
        let result = apply_binary(lhs, rhs, operator, &op_location, env);
        assert_eq!(result, Ok(Value::Integer(42)));
    }

    #[test]
    fn apply_binary_add_overflow() {
        let env = &mut HashMap::new();
        let lhs = Term::Value(Value::Integer(i64::MAX));
        let rhs = Term::Value(Value::Integer(1));
        let operator = BinaryOperator::Add;
        let op_location = 4..5;
        let result = apply_binary(lhs, rhs, operator, &op_location, env);
        assert_eq!(
            result,
            Err(Error {
                cause: EvalError::Overflow,
                location: 4..5,
            })
        );
    }

    #[test]
    fn apply_binary_subtract() {
        let env = &mut HashMap::new();
        let lhs = Term::Value(Value::Integer(30));
        let rhs = Term::Value(Value::Integer(12));
        let operator = BinaryOperator::Subtract;
        let op_location = 4..5;
        let result = apply_binary(lhs, rhs, operator, &op_location, env);
        assert_eq!(result, Ok(Value::Integer(18)));
    }

    #[test]
    fn apply_binary_subtract_overflow() {
        let env = &mut HashMap::new();
        let lhs = Term::Value(Value::Integer(i64::MIN));
        let rhs = Term::Value(Value::Integer(1));
        let operator = BinaryOperator::Subtract;
        let op_location = 4..5;
        let result = apply_binary(lhs, rhs, operator, &op_location, env);
        assert_eq!(
            result,
            Err(Error {
                cause: EvalError::Overflow,
                location: 4..5,
            })
        );
    }

    #[test]
    fn apply_binary_assign() {
        let env = &mut HashMap::new();
        let lhs = Term::Variable {
            name: "foo",
            location: 1..4,
        };
        let rhs = Term::Value(Value::Integer(42));
        let operator = BinaryOperator::Assign;
        let op_location = 4..5;
        let result = apply_binary(lhs, rhs, operator, &op_location, env);
        assert_eq!(result, Ok(Value::Integer(42)));
        assert_eq!(env["foo"], "42");
    }

    #[test]
    fn apply_binary_assign_not_variable() {
        let env = &mut HashMap::new();
        let lhs = Term::Value(Value::Integer(3));
        let rhs = Term::Value(Value::Integer(42));
        let operator = BinaryOperator::Assign;
        let op_location = 4..5;
        let result = apply_binary(lhs, rhs, operator, &op_location, env);
        assert_eq!(
            result,
            Err(Error {
                cause: EvalError::AssignmentToValue,
                location: 4..5,
            })
        );
    }

    #[test]
    fn apply_binary_add_assign() {
        let env = &mut HashMap::new();
        env.insert("a".to_string(), "10".to_string());
        let lhs = Term::Variable {
            name: "a",
            location: 1..2,
        };
        let rhs = Term::Value(Value::Integer(32));
        let operator = BinaryOperator::AddAssign;
        let op_location = 4..6;
        let result = apply_binary(lhs, rhs, operator, &op_location, env);
        assert_eq!(result, Ok(Value::Integer(42)));
        assert_eq!(env["a"], "42");
    }

    #[test]
    fn apply_binary_add_assign_not_variable() {
        let env = &mut HashMap::new();
        let lhs = Term::Value(Value::Integer(3));
        let rhs = Term::Value(Value::Integer(42));
        let operator = BinaryOperator::AddAssign;
        let op_location = 4..6;
        let result = apply_binary(lhs, rhs, operator, &op_location, env);
        assert_eq!(
            result,
            Err(Error {
                cause: EvalError::AssignmentToValue,
                location: 4..6,
            })
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

    #[test]
    fn eval_postfix() {
        let env = &mut HashMap::new();
        let ast = &[
            Ast::Term(Term::Variable {
                name: "x",
                location: 0..1,
            }),
            Ast::Postfix {
                operator: PostfixOperator::Increment,
                location: 1..3,
            },
        ];
        assert_eq!(eval(ast, env), Ok(Term::Value(Value::Integer(0))));
    }

    #[test]
    fn eval_logical_or_short_circuit() {
        let env = &mut HashMap::new();
        env.insert("a".to_string(), "*".to_string());
        let ast = &[
            Ast::Term(Term::Value(Value::Integer(-1))),
            Ast::Term(Term::Variable {
                name: "a",
                location: 4..5,
            }),
            Ast::Binary {
                operator: BinaryOperator::LogicalOr,
                rhs_len: 1,
                location: 2..3,
            },
        ];
        assert_eq!(eval(ast, env), Ok(Term::Value(Value::Integer(1))));
    }

    #[test]
    fn eval_logical_or_full_evaluation() {
        let env = &mut HashMap::new();
        let ast = &[
            Ast::Term(Term::Value(Value::Integer(0))),
            Ast::Term(Term::Value(Value::Integer(2))),
            Ast::Binary {
                operator: BinaryOperator::LogicalOr,
                rhs_len: 1,
                location: 2..3,
            },
        ];
        assert_eq!(eval(ast, env), Ok(Term::Value(Value::Integer(1))));

        let env = &mut HashMap::new();
        let ast = &[
            Ast::Term(Term::Value(Value::Integer(0))),
            Ast::Term(Term::Value(Value::Integer(0))),
            Ast::Binary {
                operator: BinaryOperator::LogicalOr,
                rhs_len: 1,
                location: 2..3,
            },
        ];
        assert_eq!(eval(ast, env), Ok(Term::Value(Value::Integer(0))));
    }

    #[test]
    fn eval_logical_and_short_circuit() {
        let env = &mut HashMap::new();
        env.insert("a".to_string(), "*".to_string());
        let ast = &[
            Ast::Term(Term::Value(Value::Integer(0))),
            Ast::Term(Term::Variable {
                name: "a",
                location: 4..5,
            }),
            Ast::Binary {
                operator: BinaryOperator::LogicalAnd,
                rhs_len: 1,
                location: 2..3,
            },
        ];
        assert_eq!(eval(ast, env), Ok(Term::Value(Value::Integer(0))));
    }

    #[test]
    fn eval_logical_and_full_evaluation() {
        let env = &mut HashMap::new();
        let ast = &[
            Ast::Term(Term::Value(Value::Integer(2))),
            Ast::Term(Term::Value(Value::Integer(3))),
            Ast::Binary {
                operator: BinaryOperator::LogicalAnd,
                rhs_len: 1,
                location: 2..3,
            },
        ];
        assert_eq!(eval(ast, env), Ok(Term::Value(Value::Integer(1))));

        let env = &mut HashMap::new();
        let ast = &[
            Ast::Term(Term::Value(Value::Integer(2))),
            Ast::Term(Term::Value(Value::Integer(0))),
            Ast::Binary {
                operator: BinaryOperator::LogicalAnd,
                rhs_len: 1,
                location: 2..3,
            },
        ];
        assert_eq!(eval(ast, env), Ok(Term::Value(Value::Integer(0))));
    }

    #[test]
    fn eval_binary() {
        let env = &mut HashMap::new();
        let ast = &[
            Ast::Term(Term::Value(Value::Integer(12))),
            Ast::Term(Term::Value(Value::Integer(34))),
            Ast::Binary {
                operator: BinaryOperator::Add,
                rhs_len: 1,
                location: 2..3,
            },
        ];
        assert_eq!(eval(ast, env), Ok(Term::Value(Value::Integer(46))));
    }

    #[test]
    fn eval_conditional_then() {
        let env = &mut HashMap::new();
        env.insert("a".to_string(), "*".to_string());
        let ast = &[
            Ast::Term(Term::Value(Value::Integer(1))),
            Ast::Term(Term::Value(Value::Integer(10))),
            Ast::Term(Term::Variable {
                name: "a",
                location: 4..5,
            }),
            Ast::Conditional {
                then_len: 1,
                else_len: 1,
            },
        ];
        assert_eq!(eval(ast, env), Ok(Term::Value(Value::Integer(10))));
    }

    #[test]
    fn eval_conditional_else() {
        let env = &mut HashMap::new();
        env.insert("a".to_string(), "*".to_string());
        let ast = &[
            Ast::Term(Term::Value(Value::Integer(0))),
            Ast::Term(Term::Variable {
                name: "a",
                location: 4..5,
            }),
            Ast::Term(Term::Value(Value::Integer(21))),
            Ast::Conditional {
                then_len: 1,
                else_len: 1,
            },
        ];
        assert_eq!(eval(ast, env), Ok(Term::Value(Value::Integer(21))));
    }
}
