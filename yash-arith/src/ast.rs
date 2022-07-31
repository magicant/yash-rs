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
use assert_matches::assert_matches;
use std::iter::Peekable;
use std::ops::Range;

/// Prefix operator kind
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PrefixOperator {
    /// `++`
    Increment,
    /// `--`
    Decrement,
    /// `+`
    NumericCoercion,
    /// `-`
    NumericNegation,
    /// `!`
    LogicalNegation,
    /// `~`
    BitwiseNegation,
}

/// Postfix operator kind
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PostfixOperator {
    /// `++`
    Increment,
    /// `--`
    Decrement,
}

/// Postfix operator kind
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BinaryOperator {
    /// `=`
    Assign,
    /// `||`
    ConditionalOr,
    /// `&&`
    ConditionalAnd,
    /// `|`
    BitwiseOr,
    /// `|=`
    BitwiseOrAssign,
    /// `^`
    BitwiseXor,
    /// `^=`
    BitwiseXorAssign,
    /// `&`
    BitwiseAnd,
    /// `&=`
    BitwiseAndAssign,
    /// `==`
    Equal,
    /// `!=`
    NotEqual,
    /// `<`
    LessThan,
    /// `>`
    GreaterThan,
    /// `<=`
    LessThanOrEqual,
    /// `>=`
    GreaterThanOrEqual,
    /// `<<`
    ShiftLeft,
    /// `<<=`
    ShiftLeftAssign,
    /// `>>`
    ShiftRight,
    /// `>>=`
    ShiftRightAssign,
    /// `+`
    Add,
    /// `+=`
    AddAssign,
    /// `-`
    Subtract,
    /// `-=`
    SubtractAssign,
    /// `*`
    Multiply,
    /// `*=`
    MultiplyAssign,
    /// `/`
    Divide,
    /// `/=`
    DivideAssign,
    /// `%`
    Remainder,
    /// `%=`
    RemainderAssign,
}

/// Node of an abstract syntax tree (AST)
///
/// A whole AST is meant to be constructed as a vector of `Ast` nodes. A
/// non-leaf node immediately follows its operand node in the vector. If a node
/// has more than one operand, the first operand immediately precedes the
/// second. This scheme makes up the tree in reverse Polish notation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Ast<'a> {
    /// Term: a constant value or variable
    Term(Term<'a>),
    /// Prefix operator
    ///
    /// This node immediately follows its operand node.
    Prefix {
        /// Operator
        operator: PrefixOperator,
        /// Range of the substring where the operator occurs in the parsed expression
        location: Range<usize>,
    },
    /// Postfix operator
    ///
    /// This node immediately follows its operand node.
    Postfix {
        /// Operator
        operator: PostfixOperator,
        /// Range of the substring where the operator occurs in the parsed expression
        location: Range<usize>,
    },
    /// Binary operator
    ///
    /// This node immediately follows its right-hand-side operand node. The
    /// right-hand-side operand tree in turn follows the left-hand-side.
    Binary {
        /// Operator
        operator: BinaryOperator,
        /// Length (number of `Ast` nodes) of the right-hand-side operand tree
        rhs_len: usize,
        /// Range of the substring where the operator occurs in the parsed expression
        location: Range<usize>,
    },
    /// Conditional ternary operator
    ///
    /// This node has three child nodes: the condition, the then value, and the
    /// else value. They appear in the `Ast` vector in this order and are
    /// immediately followed by this `Conditional` node.
    Conditional {
        /// Length (number of `Ast` nodes) of the then value tree
        then_len: usize,
        /// Length (number of `Ast` nodes) of the else value tree
        else_len: usize,
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
    while let Some(&Ok(Token::Operator { operator, .. })) = tokens.peek() {
        let operator = match operator {
            Operator::PlusPlus => PostfixOperator::Increment,
            Operator::MinusMinus => PostfixOperator::Decrement,
            _ => break,
        };
        let location =
            assert_matches!(tokens.next(), Some(Ok(Token::Operator { location, .. })) => location);
        result.push(Ast::Postfix { operator, location });
    }
    return Ok(());
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

        // TODO Parentheses
        Token::Operator { operator, location } => {
            let operator = match operator {
                Operator::PlusPlus => PrefixOperator::Increment,
                Operator::MinusMinus => PrefixOperator::Decrement,
                Operator::Plus => PrefixOperator::NumericCoercion,
                Operator::Minus => PrefixOperator::NumericNegation,
                Operator::Bang => PrefixOperator::LogicalNegation,
                Operator::Tilde => PrefixOperator::BitwiseNegation,
                _ => todo!("handle syntax error"),
            };
            parse_leaf(tokens, result)?;
            result.push(Ast::Prefix { operator, location });
            Ok(())
        }
    }
}

/// Parses a expression that may contain binary and ternary operators.
///
/// This function consumes binary operators with precedence equal to or greater
/// than the given minimum precedence, which must be greater than 0.
fn parse_tree<'a, I>(
    tokens: &mut Peekable<I>,
    min_precedence: u8,
    result: &mut Vec<Ast<'a>>,
) -> Result<(), Error>
where
    I: Iterator<Item = Result<Token<'a>, crate::token::Error>>,
{
    parse_leaf(tokens, result)?;

    while let Some(&Ok(Token::Operator { operator, .. })) = tokens.peek() {
        let precedence = operator.precedence();
        if precedence < min_precedence {
            break;
        }

        let location =
            assert_matches!(tokens.next(), Some(Ok(Token::Operator { location, .. })) => location);

        use Operator::*;
        match operator {
            Equal => {
                let old_len = result.len();
                parse_tree(tokens, precedence, result)?;
                result.push(Ast::Binary {
                    operator: BinaryOperator::Assign,
                    rhs_len: result.len() - old_len,
                    location,
                });
            }
            _ => todo!("handle operator {:?}", operator),
        };
    }
    Ok(())
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
    parse_tree(&mut tokens, 1, &mut result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Tokens;
    use crate::token::Value;

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
        assert_eq!(
            parse_str("a++").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 0..1,
                }),
                Ast::Postfix {
                    operator: PostfixOperator::Increment,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn decrement_postfix_operator() {
        assert_eq!(
            parse_str("a--").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 0..1,
                }),
                Ast::Postfix {
                    operator: PostfixOperator::Decrement,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn combination_of_postfix_operators() {
        assert_eq!(
            parse_str(" x ++  -- ++ ").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "x",
                    location: 1..2,
                }),
                Ast::Postfix {
                    operator: PostfixOperator::Increment,
                    location: 3..5,
                },
                Ast::Postfix {
                    operator: PostfixOperator::Decrement,
                    location: 7..9,
                },
                Ast::Postfix {
                    operator: PostfixOperator::Increment,
                    location: 10..12,
                },
            ]
        );
    }

    #[test]
    fn increment_prefix_operator() {
        assert_eq!(
            parse_str("++a").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 2..3,
                }),
                Ast::Prefix {
                    operator: PrefixOperator::Increment,
                    location: 0..2,
                },
            ]
        );
    }

    #[test]
    fn decrement_prefix_operator() {
        assert_eq!(
            parse_str("--a").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 2..3,
                }),
                Ast::Prefix {
                    operator: PrefixOperator::Decrement,
                    location: 0..2,
                },
            ]
        );
    }

    #[test]
    fn numeric_coercion_prefix_operator() {
        assert_eq!(
            parse_str("+a").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 1..2,
                }),
                Ast::Prefix {
                    operator: PrefixOperator::NumericCoercion,
                    location: 0..1,
                },
            ]
        );
    }

    #[test]
    fn numeric_negation_prefix_operator() {
        assert_eq!(
            parse_str("-a").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 1..2,
                }),
                Ast::Prefix {
                    operator: PrefixOperator::NumericNegation,
                    location: 0..1,
                },
            ]
        );
    }

    #[test]
    fn logical_negation_prefix_operator() {
        assert_eq!(
            parse_str("!a").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 1..2,
                }),
                Ast::Prefix {
                    operator: PrefixOperator::LogicalNegation,
                    location: 0..1,
                },
            ]
        );
    }

    #[test]
    fn bitwise_negation_prefix_operator() {
        assert_eq!(
            parse_str("~a").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 1..2,
                }),
                Ast::Prefix {
                    operator: PrefixOperator::BitwiseNegation,
                    location: 0..1,
                },
            ]
        );
    }

    #[test]
    fn combination_of_prefix_operators() {
        assert_eq!(
            parse_str(" - + !  ~ ++ -- i ").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "i",
                    location: 16..17,
                }),
                Ast::Prefix {
                    operator: PrefixOperator::Decrement,
                    location: 13..15,
                },
                Ast::Prefix {
                    operator: PrefixOperator::Increment,
                    location: 10..12,
                },
                Ast::Prefix {
                    operator: PrefixOperator::BitwiseNegation,
                    location: 8..9,
                },
                Ast::Prefix {
                    operator: PrefixOperator::LogicalNegation,
                    location: 5..6,
                },
                Ast::Prefix {
                    operator: PrefixOperator::NumericCoercion,
                    location: 3..4,
                },
                Ast::Prefix {
                    operator: PrefixOperator::NumericNegation,
                    location: 1..2,
                },
            ]
        );
    }

    #[test]
    fn simple_assignment_operator() {
        assert_eq!(
            parse_str("a=42").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 0..1,
                }),
                Ast::Term(Term::Value(Value::Integer(42))),
                Ast::Binary {
                    operator: BinaryOperator::Assign,
                    rhs_len: 1,
                    location: 1..2,
                },
            ]
        );
    }

    // TODO binary_operators
    // TODO conditional_operator
}
