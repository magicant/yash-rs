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

use std::fmt::Debug;
use std::fmt::Display;
use std::ops::Range;

mod token;

use token::PeekableTokens;
pub use token::TokenError;
pub use token::Value;

mod ast;

pub use ast::SyntaxError;

mod env;

pub use env::Env;

mod eval;

pub use eval::EvalError;

/// Cause of an arithmetic expansion error
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ErrorCause<E> {
    /// Syntax error parsing the expression
    SyntaxError(SyntaxError),
    /// Error evaluating the parsed expression
    EvalError(EvalError<E>),
}

impl<E: Display> Display for ErrorCause<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ErrorCause::*;
        match self {
            SyntaxError(e) => Display::fmt(e, f),
            EvalError(e) => Display::fmt(e, f),
        }
    }
}

impl<E> From<TokenError> for ErrorCause<E> {
    fn from(e: TokenError) -> Self {
        ErrorCause::SyntaxError(e.into())
    }
}

impl<E> From<SyntaxError> for ErrorCause<E> {
    fn from(e: SyntaxError) -> Self {
        ErrorCause::SyntaxError(e)
    }
}

impl<E> From<EvalError<E>> for ErrorCause<E> {
    fn from(e: EvalError<E>) -> Self {
        ErrorCause::EvalError(e)
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

impl<E> From<ast::Error> for Error<E> {
    fn from(e: ast::Error) -> Self {
        Error {
            cause: e.cause.into(),
            location: e.location,
        }
    }
}

impl<E> From<eval::Error<E>> for Error<E> {
    fn from(e: eval::Error<E>) -> Self {
        Error {
            cause: e.cause.into(),
            location: e.location,
        }
    }
}

/// Performs arithmetic expansion
pub fn eval<E: Env>(expression: &str, env: &mut E) -> Result<Value, Error<E::AssignVariableError>> {
    let tokens = PeekableTokens::from(expression);
    let ast = ast::parse(tokens)?;
    let term = eval::eval(&ast, env)?;
    let value = eval::into_value(term, env)?;
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
                cause: TokenError::InvalidNumericConstant.into(),
                location: 0..2,
            })
        );
        assert_eq!(
            eval("0192", env),
            Err(Error {
                cause: TokenError::InvalidNumericConstant.into(),
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
                cause: EvalError::InvalidVariableValue("".to_string()).into(),
                location: 0..3,
            })
        );
        assert_eq!(
            eval("bar", env),
            Err(Error {
                cause: EvalError::InvalidVariableValue("*".to_string()).into(),
                location: 0..3,
            })
        );
        assert_eq!(
            eval("  oops ", env),
            Err(Error {
                cause: EvalError::InvalidVariableValue("foo".to_string()).into(),
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
                cause: EvalError::Overflow.into(),
                location: 18..20,
            })
        );
        assert_eq!(
            eval("0<<1000", env),
            Err(Error {
                cause: EvalError::Overflow.into(),
                location: 1..3,
            })
        );
        assert_eq!(
            eval("0<<0x100000000", env),
            Err(Error {
                cause: EvalError::Overflow.into(),
                location: 1..3,
            })
        );

        assert_eq!(
            eval("0>>1000", env),
            Err(Error {
                cause: EvalError::Overflow.into(),
                location: 1..3,
            })
        );
        assert_eq!(
            eval("0>>0x100000000", env),
            Err(Error {
                cause: EvalError::Overflow.into(),
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
                cause: EvalError::LeftShiftingNegative.into(),
                location: 2..4,
            })
        );
        assert_eq!(
            eval("(-0x7FFFFFFFFFFFFFFF-1)<<1", env),
            Err(Error {
                cause: EvalError::LeftShiftingNegative.into(),
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
                cause: EvalError::ReverseShifting.into(),
                location: 2..4,
            })
        );

        assert_eq!(
            eval("1 >> -1", env),
            Err(Error {
                cause: EvalError::ReverseShifting.into(),
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
                cause: EvalError::Overflow.into(),
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
                cause: EvalError::Overflow.into(),
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
                cause: EvalError::Overflow.into(),
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
                cause: EvalError::DivisionByZero.into(),
                location: 1..2,
            })
        );
        assert_eq!(
            eval("0/0", env),
            Err(Error {
                cause: EvalError::DivisionByZero.into(),
                location: 1..2,
            })
        );
        assert_eq!(
            eval("10/0", env),
            Err(Error {
                cause: EvalError::DivisionByZero.into(),
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
                cause: EvalError::Overflow.into(),
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
                cause: EvalError::DivisionByZero.into(),
                location: 1..2,
            })
        );
        assert_eq!(
            eval("0%0", env),
            Err(Error {
                cause: EvalError::DivisionByZero.into(),
                location: 1..2,
            })
        );
        assert_eq!(
            eval("10%0", env),
            Err(Error {
                cause: EvalError::DivisionByZero.into(),
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
                cause: EvalError::Overflow.into(),
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
                cause: EvalError::Overflow.into(),
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
                cause: EvalError::Overflow.into(),
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
                cause: EvalError::Overflow.into(),
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

    #[test]
    fn unmatched_parenthesis() {
        let env = &mut HashMap::new();
        assert_eq!(
            eval(" ( 1 ", env),
            Err(Error {
                cause: SyntaxError::UnclosedParenthesis.into(),
                location: 1..2,
            })
        );
    }
}
