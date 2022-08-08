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

// TODO: POSIX does not require the increment/decrement operators. Maybe we
// should provide an option to reject those non-portable operators.

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
    LogicalOr,
    /// `&&`
    LogicalAnd,
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
    EqualTo,
    /// `!=`
    NotEqualTo,
    /// `<`
    LessThan,
    /// `>`
    GreaterThan,
    /// `<=`
    LessThanOrEqualTo,
    /// `>=`
    GreaterThanOrEqualTo,
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

/// Associativity kind of binary operators
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Associativity {
    Left,
    Right,
}

impl Operator {
    fn as_prefix(self) -> Option<PrefixOperator> {
        match self {
            Operator::PlusPlus => Some(PrefixOperator::Increment),
            Operator::MinusMinus => Some(PrefixOperator::Decrement),
            Operator::Plus => Some(PrefixOperator::NumericCoercion),
            Operator::Minus => Some(PrefixOperator::NumericNegation),
            Operator::Bang => Some(PrefixOperator::LogicalNegation),
            Operator::Tilde => Some(PrefixOperator::BitwiseNegation),
            _ => None,
        }
    }

    fn as_postfix(self) -> Option<PostfixOperator> {
        match self {
            Operator::PlusPlus => Some(PostfixOperator::Increment),
            Operator::MinusMinus => Some(PostfixOperator::Decrement),
            _ => None,
        }
    }

    fn as_binary(self) -> Option<(BinaryOperator, Associativity)> {
        use Associativity::*;
        use BinaryOperator::*;
        match self {
            Operator::Equal => Some((Assign, Right)),
            Operator::BarEqual => Some((BitwiseOrAssign, Right)),
            Operator::CaretEqual => Some((BitwiseXorAssign, Right)),
            Operator::AndEqual => Some((BitwiseAndAssign, Right)),
            Operator::LessLessEqual => Some((ShiftLeftAssign, Right)),
            Operator::GreaterGreaterEqual => Some((ShiftRightAssign, Right)),
            Operator::PlusEqual => Some((AddAssign, Right)),
            Operator::MinusEqual => Some((SubtractAssign, Right)),
            Operator::AsteriskEqual => Some((MultiplyAssign, Right)),
            Operator::SlashEqual => Some((DivideAssign, Right)),
            Operator::PercentEqual => Some((RemainderAssign, Right)),
            Operator::BarBar => Some((LogicalOr, Left)),
            Operator::AndAnd => Some((LogicalAnd, Left)),
            Operator::Bar => Some((BitwiseOr, Left)),
            Operator::Caret => Some((BitwiseXor, Left)),
            Operator::And => Some((BitwiseAnd, Left)),
            Operator::EqualEqual => Some((EqualTo, Left)),
            Operator::BangEqual => Some((NotEqualTo, Left)),
            Operator::Less => Some((LessThan, Left)),
            Operator::LessEqual => Some((LessThanOrEqualTo, Left)),
            Operator::Greater => Some((GreaterThan, Left)),
            Operator::GreaterEqual => Some((GreaterThanOrEqualTo, Left)),
            Operator::LessLess => Some((ShiftLeft, Left)),
            Operator::GreaterGreater => Some((ShiftRight, Left)),
            Operator::Plus => Some((Add, Left)),
            Operator::Minus => Some((Subtract, Left)),
            Operator::Asterisk => Some((Multiply, Left)),
            Operator::Slash => Some((Divide, Left)),
            Operator::Percent => Some((Remainder, Left)),
            _ => None,
        }
    }

    /// Returns the precedence of the operator.
    ///
    /// If the operator acts as both a unary and binary operator, the result is
    /// the precedence as a binary operator.
    fn precedence(self) -> u8 {
        use Operator::*;
        match self {
            CloseParen | Colon => 0,
            Equal | BarEqual | CaretEqual | AndEqual | LessLessEqual | GreaterGreaterEqual
            | PlusEqual | MinusEqual | AsteriskEqual | SlashEqual | PercentEqual => 1,
            Question => 2,
            BarBar => 3,
            AndAnd => 4,
            Bar => 5,
            Caret => 6,
            And => 7,
            EqualEqual | BangEqual => 8,
            Less | LessEqual | Greater | GreaterEqual => 9,
            LessLess | GreaterGreater => 10,
            Plus | Minus => 11,
            Asterisk | Slash | Percent => 12,
            Tilde | Bang | PlusPlus | MinusMinus | OpenParen => 13,
        }
    }
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

/// Parses postfix operators
fn parse_postfix<'a, I>(tokens: &mut Peekable<I>, result: &mut Vec<Ast<'a>>) -> Result<(), Error>
where
    I: Iterator<Item = Result<Token<'a>, crate::token::Error>>,
{
    while let Some(&Ok(Token::Operator { operator, .. })) = tokens.peek() {
        let operator = match operator.as_postfix() {
            Some(operator) => operator,
            None => break,
        };
        let location =
            assert_matches!(tokens.next(), Some(Ok(Token::Operator { location, .. })) => location);
        result.push(Ast::Postfix { operator, location });
    }
    Ok(())
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

        Token::Operator { operator, .. } if operator == Operator::OpenParen => {
            parse_tree(tokens, 1, result)?;

            // TODO Reject if a closing parenthesis is missing
            tokens.next().transpose()?;

            parse_postfix(tokens, result)
        }

        Token::Operator { operator, location } => {
            let operator = operator.as_prefix().expect("TODO: handle syntax error");
            parse_leaf(tokens, result)?;
            result.push(Ast::Prefix { operator, location });
            Ok(())
        }
    }
}

/// Parses the right-hand-side operand of a binary operation and pushes the
/// operator to the result.
fn parse_binary_rhs<'a, I>(
    tokens: &mut Peekable<I>,
    operator: BinaryOperator,
    location: Range<usize>,
    min_precedence: u8,
    result: &mut Vec<Ast<'a>>,
) -> Result<(), Error>
where
    I: Iterator<Item = Result<Token<'a>, crate::token::Error>>,
{
    let old_len = result.len();
    parse_tree(tokens, min_precedence, result)?;
    result.push(Ast::Binary {
        operator,
        rhs_len: result.len() - old_len,
        location,
    });
    Ok(())
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
        if operator == Question {
            let then_index = result.len();
            parse_tree(tokens, 1, result)?;

            // TODO Reject if a colon is missing
            tokens.next().transpose()?;

            let else_index = result.len();
            parse_tree(tokens, precedence, result)?;

            result.push(Ast::Conditional {
                then_len: else_index - then_index,
                else_len: result.len() - else_index,
            });
            continue;
        }

        let (operator, associativity) = operator.as_binary().expect("TODO: unsupported operator");
        let rhs_precedence = match associativity {
            Associativity::Left => precedence + 1,
            Associativity::Right => precedence,
        };
        parse_binary_rhs(tokens, operator, location, rhs_precedence, result)?
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
    // TODO Reject if there are unparsed tokens
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
    fn combination_of_unary_and_binary_operators() {
        assert_eq!(
            parse_str("0 + + a ++ * !1").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(0))),
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 6..7,
                }),
                Ast::Postfix {
                    operator: PostfixOperator::Increment,
                    location: 8..10,
                },
                Ast::Prefix {
                    operator: PrefixOperator::NumericCoercion,
                    location: 4..5,
                },
                Ast::Term(Term::Value(Value::Integer(1))),
                Ast::Prefix {
                    operator: PrefixOperator::LogicalNegation,
                    location: 13..14,
                },
                Ast::Binary {
                    operator: BinaryOperator::Multiply,
                    rhs_len: 2,
                    location: 11..12,
                },
                Ast::Binary {
                    operator: BinaryOperator::Add,
                    rhs_len: 6,
                    location: 2..3,
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

    #[test]
    fn bitwise_or_assign_operator() {
        assert_eq!(
            parse_str("b|=2").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "b",
                    location: 0..1,
                }),
                Ast::Term(Term::Value(Value::Integer(2))),
                Ast::Binary {
                    operator: BinaryOperator::BitwiseOrAssign,
                    rhs_len: 1,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn bitwise_xor_assign_operator() {
        assert_eq!(
            parse_str("c^=3").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "c",
                    location: 0..1,
                }),
                Ast::Term(Term::Value(Value::Integer(3))),
                Ast::Binary {
                    operator: BinaryOperator::BitwiseXorAssign,
                    rhs_len: 1,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn bitwise_and_assign_operator() {
        assert_eq!(
            parse_str("d&=5").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "d",
                    location: 0..1,
                }),
                Ast::Term(Term::Value(Value::Integer(5))),
                Ast::Binary {
                    operator: BinaryOperator::BitwiseAndAssign,
                    rhs_len: 1,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn shift_left_assign_operator() {
        assert_eq!(
            parse_str("e<<=7").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "e",
                    location: 0..1,
                }),
                Ast::Term(Term::Value(Value::Integer(7))),
                Ast::Binary {
                    operator: BinaryOperator::ShiftLeftAssign,
                    rhs_len: 1,
                    location: 1..4,
                },
            ]
        );
    }

    #[test]
    fn shift_right_assign_operator() {
        assert_eq!(
            parse_str("f>>=11").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "f",
                    location: 0..1,
                }),
                Ast::Term(Term::Value(Value::Integer(11))),
                Ast::Binary {
                    operator: BinaryOperator::ShiftRightAssign,
                    rhs_len: 1,
                    location: 1..4,
                },
            ]
        );
    }

    #[test]
    fn add_assign_operator() {
        assert_eq!(
            parse_str("g+=13").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "g",
                    location: 0..1,
                }),
                Ast::Term(Term::Value(Value::Integer(13))),
                Ast::Binary {
                    operator: BinaryOperator::AddAssign,
                    rhs_len: 1,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn subtract_assign_operator() {
        assert_eq!(
            parse_str("h-=17").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "h",
                    location: 0..1,
                }),
                Ast::Term(Term::Value(Value::Integer(17))),
                Ast::Binary {
                    operator: BinaryOperator::SubtractAssign,
                    rhs_len: 1,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn multiply_assign_operator() {
        assert_eq!(
            parse_str("i*=19").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "i",
                    location: 0..1,
                }),
                Ast::Term(Term::Value(Value::Integer(19))),
                Ast::Binary {
                    operator: BinaryOperator::MultiplyAssign,
                    rhs_len: 1,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn divide_assign_operator() {
        assert_eq!(
            parse_str("j/=23").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "j",
                    location: 0..1,
                }),
                Ast::Term(Term::Value(Value::Integer(23))),
                Ast::Binary {
                    operator: BinaryOperator::DivideAssign,
                    rhs_len: 1,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn remainder_assign_operator() {
        assert_eq!(
            parse_str("k%=29").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "k",
                    location: 0..1,
                }),
                Ast::Term(Term::Value(Value::Integer(29))),
                Ast::Binary {
                    operator: BinaryOperator::RemainderAssign,
                    rhs_len: 1,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn assignment_operators_are_right_associative() {
        assert_eq!(
            parse_str(" a = b |= c ^= d &= e <<= f >>= g += h -= i *= j /= k %= m ").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 1..2,
                }),
                Ast::Term(Term::Variable {
                    name: "b",
                    location: 5..6,
                }),
                Ast::Term(Term::Variable {
                    name: "c",
                    location: 10..11,
                }),
                Ast::Term(Term::Variable {
                    name: "d",
                    location: 15..16,
                }),
                Ast::Term(Term::Variable {
                    name: "e",
                    location: 20..21,
                }),
                Ast::Term(Term::Variable {
                    name: "f",
                    location: 26..27,
                }),
                Ast::Term(Term::Variable {
                    name: "g",
                    location: 32..33,
                }),
                Ast::Term(Term::Variable {
                    name: "h",
                    location: 37..38,
                }),
                Ast::Term(Term::Variable {
                    name: "i",
                    location: 42..43,
                }),
                Ast::Term(Term::Variable {
                    name: "j",
                    location: 47..48,
                }),
                Ast::Term(Term::Variable {
                    name: "k",
                    location: 52..53,
                }),
                Ast::Term(Term::Variable {
                    name: "m",
                    location: 57..58,
                }),
                Ast::Binary {
                    operator: BinaryOperator::RemainderAssign,
                    rhs_len: 1,
                    location: 54..56,
                },
                Ast::Binary {
                    operator: BinaryOperator::DivideAssign,
                    rhs_len: 3,
                    location: 49..51,
                },
                Ast::Binary {
                    operator: BinaryOperator::MultiplyAssign,
                    rhs_len: 5,
                    location: 44..46,
                },
                Ast::Binary {
                    operator: BinaryOperator::SubtractAssign,
                    rhs_len: 7,
                    location: 39..41,
                },
                Ast::Binary {
                    operator: BinaryOperator::AddAssign,
                    rhs_len: 9,
                    location: 34..36,
                },
                Ast::Binary {
                    operator: BinaryOperator::ShiftRightAssign,
                    rhs_len: 11,
                    location: 28..31,
                },
                Ast::Binary {
                    operator: BinaryOperator::ShiftLeftAssign,
                    rhs_len: 13,
                    location: 22..25,
                },
                Ast::Binary {
                    operator: BinaryOperator::BitwiseAndAssign,
                    rhs_len: 15,
                    location: 17..19,
                },
                Ast::Binary {
                    operator: BinaryOperator::BitwiseXorAssign,
                    rhs_len: 17,
                    location: 12..14,
                },
                Ast::Binary {
                    operator: BinaryOperator::BitwiseOrAssign,
                    rhs_len: 19,
                    location: 7..9,
                },
                Ast::Binary {
                    operator: BinaryOperator::Assign,
                    rhs_len: 21,
                    location: 3..4,
                },
            ]
        );
    }

    #[test]
    fn logical_or_operator() {
        assert_eq!(
            parse_str("3||5").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(3))),
                Ast::Term(Term::Value(Value::Integer(5))),
                Ast::Binary {
                    operator: BinaryOperator::LogicalOr,
                    rhs_len: 1,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn logical_or_operator_is_left_associative() {
        assert_eq!(
            parse_str("1||2||3").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(1))),
                Ast::Term(Term::Value(Value::Integer(2))),
                Ast::Binary {
                    operator: BinaryOperator::LogicalOr,
                    rhs_len: 1,
                    location: 1..3,
                },
                Ast::Term(Term::Value(Value::Integer(3))),
                Ast::Binary {
                    operator: BinaryOperator::LogicalOr,
                    rhs_len: 1,
                    location: 4..6,
                },
            ]
        );
    }

    #[test]
    fn logical_or_operator_in_conditional_operator() {
        assert_eq!(
            parse_str("1||2?3:4||5").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(1))),
                Ast::Term(Term::Value(Value::Integer(2))),
                Ast::Binary {
                    operator: BinaryOperator::LogicalOr,
                    rhs_len: 1,
                    location: 1..3,
                },
                Ast::Term(Term::Value(Value::Integer(3))),
                Ast::Term(Term::Value(Value::Integer(4))),
                Ast::Term(Term::Value(Value::Integer(5))),
                Ast::Binary {
                    operator: BinaryOperator::LogicalOr,
                    rhs_len: 1,
                    location: 8..10,
                },
                Ast::Conditional {
                    then_len: 1,
                    else_len: 3,
                },
            ]
        );
    }

    #[test]
    fn logical_and_operator() {
        assert_eq!(
            parse_str("3&&5").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(3))),
                Ast::Term(Term::Value(Value::Integer(5))),
                Ast::Binary {
                    operator: BinaryOperator::LogicalAnd,
                    rhs_len: 1,
                    location: 1..3,
                },
            ]
        );
    }

    #[test]
    fn logical_and_operator_is_left_associative() {
        assert_eq!(
            parse_str("1&&2&&3").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(1))),
                Ast::Term(Term::Value(Value::Integer(2))),
                Ast::Binary {
                    operator: BinaryOperator::LogicalAnd,
                    rhs_len: 1,
                    location: 1..3,
                },
                Ast::Term(Term::Value(Value::Integer(3))),
                Ast::Binary {
                    operator: BinaryOperator::LogicalAnd,
                    rhs_len: 1,
                    location: 4..6,
                },
            ]
        );
    }

    #[test]
    fn logical_and_operator_in_logical_or_operator() {
        assert_eq!(
            parse_str("1&&2||3&&4").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(1))),
                Ast::Term(Term::Value(Value::Integer(2))),
                Ast::Binary {
                    operator: BinaryOperator::LogicalAnd,
                    rhs_len: 1,
                    location: 1..3,
                },
                Ast::Term(Term::Value(Value::Integer(3))),
                Ast::Term(Term::Value(Value::Integer(4))),
                Ast::Binary {
                    operator: BinaryOperator::LogicalAnd,
                    rhs_len: 1,
                    location: 7..9,
                },
                Ast::Binary {
                    operator: BinaryOperator::LogicalOr,
                    rhs_len: 3,
                    location: 4..6,
                },
            ]
        );
    }

    #[test]
    fn multiplication_operator_in_addition_operator() {
        assert_eq!(
            parse_str("1*2+3*4").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(1))),
                Ast::Term(Term::Value(Value::Integer(2))),
                Ast::Binary {
                    operator: BinaryOperator::Multiply,
                    rhs_len: 1,
                    location: 1..2,
                },
                Ast::Term(Term::Value(Value::Integer(3))),
                Ast::Term(Term::Value(Value::Integer(4))),
                Ast::Binary {
                    operator: BinaryOperator::Multiply,
                    rhs_len: 1,
                    location: 5..6,
                },
                Ast::Binary {
                    operator: BinaryOperator::Add,
                    rhs_len: 3,
                    location: 3..4,
                },
            ]
        );
    }

    #[test]
    fn multiplication_division_remainder_operators_are_left_associative() {
        assert_eq!(
            parse_str("1*2/3%4").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(1))),
                Ast::Term(Term::Value(Value::Integer(2))),
                Ast::Binary {
                    operator: BinaryOperator::Multiply,
                    rhs_len: 1,
                    location: 1..2,
                },
                Ast::Term(Term::Value(Value::Integer(3))),
                Ast::Binary {
                    operator: BinaryOperator::Divide,
                    rhs_len: 1,
                    location: 3..4,
                },
                Ast::Term(Term::Value(Value::Integer(4))),
                Ast::Binary {
                    operator: BinaryOperator::Remainder,
                    rhs_len: 1,
                    location: 5..6,
                },
            ]
        );
    }

    #[test]
    fn conditional_operator() {
        assert_eq!(
            parse_str("1?2:3").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(1))),
                Ast::Term(Term::Value(Value::Integer(2))),
                Ast::Term(Term::Value(Value::Integer(3))),
                Ast::Conditional {
                    then_len: 1,
                    else_len: 1,
                },
            ]
        );
    }

    #[test]
    fn assignment_in_then_value() {
        assert_eq!(
            parse_str("a ? b = 0 : 1").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 0..1,
                }),
                Ast::Term(Term::Variable {
                    name: "b",
                    location: 4..5,
                }),
                Ast::Term(Term::Value(Value::Integer(0))),
                Ast::Binary {
                    operator: BinaryOperator::Assign,
                    rhs_len: 1,
                    location: 6..7,
                },
                Ast::Term(Term::Value(Value::Integer(1))),
                Ast::Conditional {
                    then_len: 3,
                    else_len: 1,
                },
            ]
        );
    }

    #[test]
    fn condition_in_assignment() {
        assert_eq!(
            parse_str("4 ? a : b = 5").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(4))),
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 4..5,
                }),
                Ast::Term(Term::Variable {
                    name: "b",
                    location: 8..9,
                }),
                Ast::Conditional {
                    then_len: 1,
                    else_len: 1,
                },
                Ast::Term(Term::Value(Value::Integer(5))),
                Ast::Binary {
                    operator: BinaryOperator::Assign,
                    rhs_len: 1,
                    location: 10..11,
                },
            ]
        );
    }

    #[test]
    fn conditional_operator_is_right_associative() {
        assert_eq!(
            parse_str("5 ? 6 : 7 ? 8 : 9").unwrap(),
            [
                Ast::Term(Term::Value(Value::Integer(5))),
                Ast::Term(Term::Value(Value::Integer(6))),
                Ast::Term(Term::Value(Value::Integer(7))),
                Ast::Term(Term::Value(Value::Integer(8))),
                Ast::Term(Term::Value(Value::Integer(9))),
                Ast::Conditional {
                    then_len: 1,
                    else_len: 1,
                },
                Ast::Conditional {
                    then_len: 1,
                    else_len: 4,
                },
            ]
        );
    }

    // TODO question_without_colon
    // TODO colon_without_question

    #[test]
    fn parentheses() {
        assert_eq!(
            parse_str("(a = 0)--").unwrap(),
            [
                Ast::Term(Term::Variable {
                    name: "a",
                    location: 1..2,
                }),
                Ast::Term(Term::Value(Value::Integer(0))),
                Ast::Binary {
                    operator: BinaryOperator::Assign,
                    rhs_len: 1,
                    location: 3..4,
                },
                Ast::Postfix {
                    operator: PostfixOperator::Decrement,
                    location: 7..9,
                },
            ]
        );
    }

    // TODO unmatched_parentheses
}
