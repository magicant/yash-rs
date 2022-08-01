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
        Ast::Prefix { operator, location } => todo!(),
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
}
