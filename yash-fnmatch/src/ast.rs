// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! Abstract syntax tree for globbing patterns

use crate::Config;
use crate::Error;
use crate::PatternChar;
use std::ops::RangeInclusive;

/// Bracket expression component
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BracketAtom {
    /// Literal character
    Char(char),
    /// Collating symbol (`[.x.]`)
    CollatingSymbol(String),
    /// Equivalence Class (`[=x=]`)
    EquivalenceClass(String),
}

impl From<char> for BracketAtom {
    fn from(c: char) -> Self {
        BracketAtom::Char(c)
    }
}

impl BracketAtom {
    /// Parses an inner bracket expression (except the initial '[').
    ///
    /// This function parses a collating symbol, equivalence class, or character
    /// class.
    ///
    /// If successful, returns the result as well as an iterator that yields
    /// characters following the closing bracket. Returns `Ok(None)` if the
    /// inner bracket expression is not valid.
    fn parse_inner<I>(mut i: I) -> Result<Option<(Self, I)>, Error>
    where
        I: Iterator<Item = PatternChar>,
    {
        match i.next() {
            // TODO Check PatternChar type
            Some(pc) if pc.char_value() == '.' => {
                let mut value = String::new();
                while let Some(pc) = i.next() {
                    value.push(pc.char_value());
                    if value.ends_with(".]") {
                        value.truncate(value.len() - 2);
                        return Ok(Some((BracketAtom::CollatingSymbol(value), i)));
                    }
                }
                Ok(None)
            }
            Some(pc) if pc.char_value() == '=' => {
                let mut value = String::new();
                while let Some(pc) = i.next() {
                    value.push(pc.char_value());
                    if value.ends_with("=]") {
                        value.truncate(value.len() - 2);
                        return Ok(Some((BracketAtom::EquivalenceClass(value), i)));
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}

/// Bracket expression component
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BracketItem {
    /// Atom
    Atom(BracketAtom),
    /// Character range
    Range(RangeInclusive<BracketAtom>),
}

impl<T: Into<BracketAtom>> From<T> for BracketItem {
    fn from(value: T) -> Self {
        BracketItem::Atom(value.into())
    }
}
impl From<RangeInclusive<BracketAtom>> for BracketItem {
    fn from(range: RangeInclusive<BracketAtom>) -> Self {
        BracketItem::Range(range)
    }
}

/// Bracket expression
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bracket {
    /// Whether there is an initial `!`
    ///
    /// When the bracket expression starts with an `!`, the set of matching
    /// character is inverted.
    pub complement: bool,

    /// Content of the bracket expression
    pub items: Vec<BracketItem>,
}

impl Bracket {
    /// Parses a bracket expression (except the initial '[').
    ///
    /// If successful, returns the result as well as an iterator that yields
    /// characters following the bracket expression. Returns `Ok(None)` if a
    /// bracket expression is not found.
    fn parse<I>(mut i: I, _config: Config) -> Result<Option<(Self, I)>, Error>
    where
        I: Iterator<Item = PatternChar> + Clone,
    {
        use BracketAtom::*;
        use BracketItem::*;

        let mut bracket = Bracket {
            complement: false,
            items: Vec::new(),
        };
        while let Some(pc) = i.next() {
            // TODO Check PatternChar type
            match pc.char_value() {
                ']' if !bracket.items.is_empty() => return Ok(Some((bracket, i))),
                '!' | '^' if !bracket.complement && bracket.items.is_empty() => {
                    bracket.complement = true
                }
                '[' => {
                    if let Some((atom, j)) = BracketAtom::parse_inner(i.clone())? {
                        bracket.items.push(atom.into());
                        i = j;
                    } else {
                        bracket.items.push(Atom(Char('[')));
                    }
                }
                c => match (bracket.items.pop(), bracket.items.pop()) {
                    (Some(Atom(Char('-'))), Some(Atom(start))) => {
                        bracket.items.push(Range(start..=Char(c)));
                    }
                    (last_1, last_2) => {
                        if let Some(last_2) = last_2 {
                            bracket.items.push(last_2);
                        }
                        if let Some(last_1) = last_1 {
                            bracket.items.push(last_1);
                        }
                        bracket.items.push(Atom(Char(c)));
                    }
                },
            }
        }
        Ok(None)
    }
}

/// Pattern component
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Atom {
    /// Literal character
    Char(char),
    /// Pattern that matches a single character (`?`)
    AnyChar,
    /// Pattern that matches any string (`*`)
    AnyString,
    /// Bracket expression
    Bracket(Bracket),
}

impl Atom {
    fn parse<I>(mut i: I, config: Config) -> Result<Option<(Self, I)>, Error>
    where
        I: Iterator<Item = PatternChar> + Clone,
    {
        if let Some(pc) = i.next() {
            // TODO Check PatternChar type
            let atom = match pc.char_value() {
                '?' => Atom::AnyChar,
                '*' => Atom::AnyString,
                '[' => {
                    if let Some((bracket, j)) = Bracket::parse(i.clone(), config)? {
                        i = j;
                        Atom::Bracket(bracket)
                    } else {
                        Atom::Char('[')
                    }
                }
                c => Atom::Char(c),
            };
            Ok(Some((atom, i)))
        } else {
            Ok(None)
        }
    }
}

/// Abstract syntax tree for a whole pattern
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Ast {
    /// Content of the pattern
    pub atoms: Vec<Atom>,
}

impl Ast {
    /// Creates a pattern with defaulted configuration.
    #[inline]
    pub fn new<I>(pattern: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = PatternChar>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        Self::with_config(pattern, Config::default())
    }

    /// Creates a pattern with a specified configuration.
    pub fn with_config<I>(pattern: I, config: Config) -> Result<Self, Error>
    where
        I: IntoIterator<Item = PatternChar>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        let mut atoms = Vec::new();
        let mut i = pattern.into_iter();
        while let Some((atom, j)) = Atom::parse(i, config)? {
            atoms.push(atom);
            i = j;
        }
        Ok(Ast { atoms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::without_escape;

    #[test]
    fn empty_pattern() {
        let ast = Ast::new(without_escape("")).unwrap();
        assert_eq!(ast.atoms, []);
    }

    #[test]
    fn single_character_pattern() {
        let ast = Ast::new(without_escape("a")).unwrap();
        assert_eq!(ast.atoms, [Atom::Char('a')]);

        let ast = Ast::new(without_escape("0")).unwrap();
        assert_eq!(ast.atoms, [Atom::Char('0')]);
    }

    #[test]
    fn double_character_pattern() {
        let ast = Ast::new(without_escape("in")).unwrap();
        assert_eq!(ast.atoms, [Atom::Char('i'), Atom::Char('n')]);
    }

    #[test]
    fn any_character_pattern() {
        let ast = Ast::new(without_escape("?")).unwrap();
        assert_eq!(ast.atoms, [Atom::AnyChar]);
    }

    #[test]
    fn any_string_pattern() {
        let ast = Ast::new(without_escape("*")).unwrap();
        assert_eq!(ast.atoms, [Atom::AnyString]);
    }

    // TODO unmatched bracket [a
    // TODO unmatched bracket a]
    // TODO unmatched bracket ][

    #[test]
    fn empty_bracket_expression() {
        let ast = Ast::new(without_escape("[]")).unwrap();
        assert_eq!(ast.atoms, [Atom::Char('['), Atom::Char(']')]);
    }

    // TODO unmatched bracket after another bracket [a][a

    #[test]
    fn single_character_bracket_expression_pattern() {
        let ast = Ast::new(without_escape("[a]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::Char('a'))]
            })]
        );
    }

    #[test]
    fn multi_character_bracket_expression_pattern() {
        let ast = Ast::new(without_escape("[xyz]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('x')),
                    BracketItem::Atom(BracketAtom::Char('y')),
                    BracketItem::Atom(BracketAtom::Char('z')),
                ]
            })]
        );
    }

    #[test]
    fn brackets_in_bracket_expression() {
        let ast = Ast::new(without_escape("[]a[]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char(']')),
                    BracketItem::Atom(BracketAtom::Char('a')),
                    BracketItem::Atom(BracketAtom::Char('[')),
                ]
            })]
        );
    }

    #[test]
    fn bracket_expression_complement_with_exclamation() {
        let ast = Ast::new(without_escape("[!12]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('1')),
                    BracketItem::Atom(BracketAtom::Char('2')),
                ]
            })]
        );
    }

    #[test]
    fn exclamation_in_bracket_expression() {
        let ast = Ast::new(without_escape("[12!]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('1')),
                    BracketItem::Atom(BracketAtom::Char('2')),
                    BracketItem::Atom(BracketAtom::Char('!')),
                ]
            })]
        );
    }

    #[test]
    fn exclamation_in_bracket_expression_complement() {
        let ast = Ast::new(without_escape("[!!]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![BracketItem::Atom(BracketAtom::Char('!'))]
            })]
        );
    }

    #[test]
    fn bracket_expression_complement_with_caret() {
        let ast = Ast::new(without_escape("[^34]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('3')),
                    BracketItem::Atom(BracketAtom::Char('4')),
                ]
            })]
        );

        let ast = Ast::new(without_escape("[^]a]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char(']')),
                    BracketItem::Atom(BracketAtom::Char('a')),
                ]
            })]
        );

        let ast = Ast::new(without_escape("[^^]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![BracketItem::Atom(BracketAtom::Char('^'))]
            })]
        );
    }

    #[test]
    fn character_range() {
        let ast = Ast::new(without_escape("[a-z]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Range(
                    BracketAtom::Char('a')..=BracketAtom::Char('z')
                )]
            })]
        );
    }

    #[test]
    fn dash_at_start_of_bracket_expression() {
        let ast = Ast::new(without_escape("[-a]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('-')),
                    BracketItem::Atom(BracketAtom::Char('a')),
                ]
            })]
        );

        let ast = Ast::new(without_escape("[!-b]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('-')),
                    BracketItem::Atom(BracketAtom::Char('b')),
                ]
            })]
        );
    }

    #[test]
    fn dash_at_end_of_bracket_expression() {
        let ast = Ast::new(without_escape("[5-]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('5')),
                    BracketItem::Atom(BracketAtom::Char('-')),
                ]
            })]
        );
    }

    #[test]
    fn ambiguous_character_range() {
        let ast = Ast::new(without_escape("[2-4-6-8]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Range(BracketAtom::Char('2')..=BracketAtom::Char('4')),
                    BracketItem::Atom(BracketAtom::Char('-')),
                    BracketItem::Range(BracketAtom::Char('6')..=BracketAtom::Char('8')),
                ]
            })]
        );
    }

    #[test]
    fn double_dash_at_start_of_bracket_expression() {
        let ast = Ast::new(without_escape("[--0]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Range(
                    BracketAtom::Char('-')..=BracketAtom::Char('0')
                )]
            })]
        );
    }

    #[test]
    fn double_dash_at_end_of_bracket_expression() {
        let ast = Ast::new(without_escape("[+--]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Range(
                    BracketAtom::Char('+')..=BracketAtom::Char('-')
                )]
            })]
        );
    }

    #[test]
    fn single_character_collating_symbol() {
        let ast = Ast::new(without_escape("[[.a.]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::CollatingSymbol(
                    "a".to_string()
                ))]
            })]
        );

        let ast = Ast::new(without_escape("[[.].]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::CollatingSymbol(
                    "]".to_string()
                ))]
            })]
        );
    }

    #[test]
    fn multi_character_collating_symbol() {
        let ast = Ast::new(without_escape("[[.ch.]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::CollatingSymbol(
                    "ch".to_string()
                ))]
            })]
        );
    }

    #[test]
    fn single_character_equivalence_class() {
        let ast = Ast::new(without_escape("[[=a=]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::EquivalenceClass(
                    "a".to_string()
                ))]
            })]
        );

        let ast = Ast::new(without_escape("[[=]=]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::EquivalenceClass(
                    "]".to_string()
                ))]
            })]
        );
    }

    #[test]
    fn multi_character_equivalence_class() {
        let ast = Ast::new(without_escape("[[=ch=]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::EquivalenceClass(
                    "ch".to_string()
                ))]
            })]
        );
    }

    // TODO collating_symbol_in_character_range

    // TODO character_class_alnum
    // TODO character_class_alpha
    // TODO character_class_blank
    // TODO character_class_cntrl
    // TODO character_class_digit
    // TODO character_class_graph
    // TODO character_class_lower
    // TODO character_class_print
    // TODO character_class_punct
    // TODO character_class_space
    // TODO character_class_upper
    // TODO character_class_xdigit
    // TODO undefined_character_class

    // TODO Config
    // TODO PatternChar Normal vs Literal
}
