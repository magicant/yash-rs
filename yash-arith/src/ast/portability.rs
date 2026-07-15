// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
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

//! Items for verifying portability of parsed expressions

use super::Ast;
use super::PostfixOperator;
use super::PrefixOperator;
use std::ops::Range;
use thiserror::Error;

/// Cause of an error because an expression contains a non-portable construct
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
#[non_exhaustive]
pub enum PortabilityError {
    /// An increment or decrement operator is used while the `portable` option
    /// is on.
    #[error("the increment and decrement operators are not portable")]
    IncrementDecrement,
}

/// Description of a non-portable construct found in an expression
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
#[error("{cause}")]
pub struct Error {
    /// Cause of the error
    pub cause: PortabilityError,
    /// Range of the non-portable construct in the parsed expression
    pub location: Range<usize>,
}

/// Checks that the parsed expression contains no non-portable constructs.
pub fn check(ast: &[Ast<'_>]) -> Result<(), Error> {
    let location = ast
        .iter()
        .filter_map(|node| match node {
            Ast::Prefix {
                operator: PrefixOperator::Increment | PrefixOperator::Decrement,
                location,
            }
            | Ast::Postfix {
                operator: PostfixOperator::Increment | PostfixOperator::Decrement,
                location,
            } => Some(location),
            _ => None,
        })
        .min_by_key(|location| location.start);

    match location {
        Some(location) => Err(Error {
            cause: PortabilityError::IncrementDecrement,
            location: location.clone(),
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parse;
    use crate::token::PeekableTokens;

    fn check_expression(expression: &str) -> Result<(), Error> {
        let ast = parse(PeekableTokens::from(expression)).unwrap();
        check(&ast)
    }

    #[test]
    fn portable_expression() {
        assert_eq!(check_expression("foo = -(1 + 2)"), Ok(()));
    }

    #[test]
    fn prefix_increment_and_decrement() {
        for (expression, location) in [("  ++foo", 2..4), ("--bar  ", 0..2)] {
            assert_eq!(
                check_expression(expression),
                Err(Error {
                    cause: PortabilityError::IncrementDecrement,
                    location,
                })
            );
        }
    }

    #[test]
    fn postfix_increment_and_decrement() {
        for (expression, location) in [("  foo++", 5..7), ("bar--  ", 3..5)] {
            assert_eq!(
                check_expression(expression),
                Err(Error {
                    cause: PortabilityError::IncrementDecrement,
                    location,
                })
            );
        }
    }

    #[test]
    fn non_portable_operator_in_unevaluated_operand() {
        assert_eq!(
            check_expression("1 || foo++"),
            Err(Error {
                cause: PortabilityError::IncrementDecrement,
                location: 8..10,
            })
        );
    }

    #[test]
    fn first_non_portable_operator_in_source_order() {
        // Prefix operators appear in the AST from the innermost to the
        // outermost, which is the reverse of their order in the source.
        assert_eq!(
            check_expression("++--foo + bar++"),
            Err(Error {
                cause: PortabilityError::IncrementDecrement,
                location: 0..2,
            })
        );
    }
}
