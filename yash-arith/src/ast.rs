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

//! Abstract syntax tree parser

use crate::token::Operator;
use crate::token::Term;
use crate::token::Token;
use crate::token::TokenError;
use std::iter::Peekable;
use std::ops::Range;

/// Postfix operator kind
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PostfixOperator {
    /// `++`
    Increment,
    /// `--`
    Decrement,
}

/// Node of an abstract syntax tree (AST)
///
/// A whole AST is meant to be constructed as a vector of `Ast` nodes. Each node
/// refers to its operand nodes by indexing into the vector rather than using
/// references or boxes to reduce allocation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Ast<'a> {
    /// Term: a constant value or variable
    Term(Term<'a>),
    /// Prefix operator
    Prefix {
        /// Operator
        operator: Operator,
        /// Index of the operand node
        operand: usize,
        /// Range of the substring where the operator occurs in the parsed expression
        location: Range<usize>,
    },
    /// Postfix operator
    Postfix {
        /// Operator
        operator: PostfixOperator,
        /// Index of the operand node
        operand: usize,
        /// Range of the substring where the operator occurs in the parsed expression
        location: Range<usize>,
    },
    /// Binary operator
    Binary {
        /// Operator
        operator: Operator,
        /// Index of the left-hand-side node
        lhs: usize,
        /// Index of the right-hand-side node
        rhs: usize,
        /// Range of the substring where the operator occurs in the parsed expression
        location: Range<usize>,
    },
    /// Conditional ternary operator
    Conditional {
        /// Index of the first operand (condition) node.
        condition_node: usize,
        /// Index of the second operand (then value) node.
        then_node: usize,
        /// Index of the third operand (else value) node.
        else_node: usize,
    },
}

/// Cause of a syntax error
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SyntaxError {
    /// Error in tokenization
    TokenError(TokenError),
    // TODO
}

impl From<TokenError> for SyntaxError {
    fn from(e: TokenError) -> Self {
        SyntaxError::TokenError(e)
    }
}

/// Description of an error that occurred during expansion
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Error {
    /// Cause of the error
    pub cause: SyntaxError,
    /// Range of the substring in the evaluated expression string where the error occurred
    pub location: Range<usize>,
}

impl From<crate::token::Error> for Error {
    fn from(e: crate::token::Error) -> Self {
        Error {
            cause: e.cause.into(),
            location: e.location,
        }
    }
}

/// Returns the index of the last node.
///
/// Panics if the slice is empty.
fn current_root_index(nodes: &[Ast]) -> usize {
    let len = nodes.len();
    assert!(len > 0);
    len - 1
}

/// Parses postfix operators
fn parse_postfix<'a, I>(tokens: &mut Peekable<I>, result: &mut Vec<Ast<'a>>) -> Result<(), Error>
where
    I: Iterator<Item = Result<Token<'a>, crate::token::Error>>,
{
    loop {
        match tokens.peek() {
            Some(Ok(Token::Operator {
                operator: Operator::PlusPlus,
                ref location,
            })) => {
                let location = location.clone();
                tokens.next();
                result.push(Ast::Postfix {
                    operator: PostfixOperator::Increment,
                    operand: current_root_index(&result),
                    location,
                });
            }

            Some(Ok(Token::Operator {
                operator: Operator::MinusMinus,
                ref location,
            })) => {
                let location = location.clone();
                tokens.next();
                result.push(Ast::Postfix {
                    operator: PostfixOperator::Decrement,
                    operand: current_root_index(&result),
                    location,
                });
            }

            _ => break Ok(()),
        }
    }
}

/// Parses a leaf expression.
///
/// A leaf expression is a term or parenthesized expression, optionally modified
/// by unary operators.
fn parse_leaf<'a, I>(tokens: &mut Peekable<I>, result: &mut Vec<Ast<'a>>) -> Result<(), Error>
where
    I: Iterator<Item = Result<Token<'a>, crate::token::Error>>,
{
    let token = tokens
        .next()
        .transpose()?
        .expect("TODO: handle empty expression error");
    match token {
        Token::Term(term) => {
            result.push(Ast::Term(term));
            parse_postfix(tokens, result)
        }
        Token::Operator { .. } => todo!("parse prefix operators"),
    }
}

/// Parses the whole expression.
///
/// A successful parse is returned as a non-empty vector of `Ast` nodes, where
/// the last node is the root.
pub fn parse<'a, I>(mut tokens: Peekable<I>) -> Result<Vec<Ast<'a>>, Error>
where
    I: Iterator<Item = Result<Token<'a>, crate::token::Error>>,
{
    let mut result = Vec::new();
    parse_leaf(&mut tokens, &mut result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Tokens;
    use crate::token::Value;
    use assert_matches::assert_matches;

    fn parse_str(source: &str) -> Result<Vec<Ast>, Error> {
        parse(Tokens::new(source).peekable())
    }

    #[test]
    fn term() {
        assert_eq!(
            parse_str("123").unwrap(),
            [Ast::Term(Term::Value(Value::Integer(123)))]
        );
        assert_eq!(
            parse_str("0x42").unwrap(),
            [Ast::Term(Term::Value(Value::Integer(0x42)))]
        );
        assert_eq!(
            parse_str(" foo ").unwrap(),
            [Ast::Term(Term::Variable {
                name: "foo",
                location: 1..4
            })]
        );
    }

    #[test]
    fn token_error_in_term() {
        assert_eq!(
            parse_str("08"),
            Err(Error {
                cause: SyntaxError::TokenError(TokenError::InvalidNumericConstant),
                location: 0..2
            })
        );
    }

    // TODO parentheses
    // TODO unmatched_parentheses

    #[test]
    fn increment_postfix_operator() {
        let nodes = parse_str("a++").unwrap();
        assert_matches!(nodes.last(), Some(&Ast::Postfix { operator, operand, ref location }) => {
            assert_eq!(operator, PostfixOperator::Increment);
            assert_eq!(*location, 1..3);
            assert_matches!(nodes[operand], Ast::Term(Term::Variable { name, ref location }) => {
                assert_eq!(name, "a");
                assert_eq!(*location, 0..1);
            });
        });
    }

    #[test]
    fn decrement_postfix_operator() {
        let nodes = parse_str("a--").unwrap();
        assert_matches!(nodes.last(), Some(&Ast::Postfix { operator, operand, ref location }) => {
            assert_eq!(operator, PostfixOperator::Decrement);
            assert_eq!(*location, 1..3);
            assert_matches!(nodes[operand], Ast::Term(Term::Variable { name, ref location }) => {
                assert_eq!(name, "a");
                assert_eq!(*location, 0..1);
            });
        });
    }

    #[test]
    fn combination_of_postfix_operators() {
        let nodes = parse_str(" x ++  -- ++ ").unwrap();
        assert_matches!(nodes.last(), Some(&Ast::Postfix { operator, operand, ref location }) => {
            assert_eq!(operator, PostfixOperator::Increment);
            assert_eq!(*location, 10..12);
            assert_matches!(nodes[operand], Ast::Postfix { operator, operand, ref location } => {
                assert_eq!(operator, PostfixOperator::Decrement);
                assert_eq!(*location, 7..9);
                assert_matches!(nodes[operand], Ast::Postfix { operator, operand, ref location } => {
                    assert_eq!(operator, PostfixOperator::Increment);
                    assert_eq!(*location, 3..5);
                    assert_matches!(nodes[operand], Ast::Term(Term::Variable { name, ref location }) => {
                        assert_eq!(name, "x");
                        assert_eq!(*location, 1..2);
                    });
                });
            });
        });
    }

    // TODO prefix_operators
    // TODO binary_operators
    // TODO conditional_operator
}
