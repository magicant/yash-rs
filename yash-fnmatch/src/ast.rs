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
    /// Character range
    Range(RangeInclusive<char>),
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
    pub atoms: Vec<BracketAtom>,
}

impl Bracket {
    fn parse<I>(mut i: I, _config: Config) -> Result<Option<(Self, I)>, Error>
    where
        I: Iterator<Item = PatternChar> + Clone,
    {
        let mut bracket = Bracket {
            complement: false,
            atoms: Vec::new(),
        };
        while let Some(pc) = i.next() {
            match pc.char_value() {
                ']' if !bracket.atoms.is_empty() => return Ok(Some((bracket, i))),
                '!' | '^' if !bracket.complement && bracket.atoms.is_empty() => {
                    bracket.complement = true
                }
                '-' => {
                    if let Some(last) = bracket.atoms.pop() {
                        if let Some(end) = i.next() {
                            match last {
                                BracketAtom::Char(start) => {
                                    let end = end.char_value();
                                    bracket.atoms.push(BracketAtom::Range(start..=end));
                                }
                                _ => todo!(),
                            }
                        } else {
                            todo!()
                        }
                    } else {
                        bracket.atoms.push(BracketAtom::Char('-'));
                    }
                }
                c => bracket.atoms.push(BracketAtom::Char(c)),
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
                atoms: vec![BracketAtom::Char('a')]
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
                atoms: vec![
                    BracketAtom::Char('x'),
                    BracketAtom::Char('y'),
                    BracketAtom::Char('z'),
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
                atoms: vec![
                    BracketAtom::Char(']'),
                    BracketAtom::Char('a'),
                    BracketAtom::Char('['),
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
                atoms: vec![BracketAtom::Char('1'), BracketAtom::Char('2')]
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
                atoms: vec![
                    BracketAtom::Char('1'),
                    BracketAtom::Char('2'),
                    BracketAtom::Char('!'),
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
                atoms: vec![BracketAtom::Char('!')]
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
                atoms: vec![BracketAtom::Char('3'), BracketAtom::Char('4')]
            })]
        );

        let ast = Ast::new(without_escape("[^]a]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                atoms: vec![BracketAtom::Char(']'), BracketAtom::Char('a')]
            })]
        );

        let ast = Ast::new(without_escape("[^^]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                atoms: vec![BracketAtom::Char('^')]
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
                atoms: vec![BracketAtom::Range('a'..='z')]
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
                atoms: vec![BracketAtom::Char('-'), BracketAtom::Char('a')]
            })]
        );

        let ast = Ast::new(without_escape("[!-b]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                atoms: vec![BracketAtom::Char('-'), BracketAtom::Char('b')]
            })]
        );
    }

    #[test]
    #[ignore]
    fn dash_at_end_of_bracket_expression() {
        let ast = Ast::new(without_escape("[5-]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                atoms: vec![BracketAtom::Char('5'), BracketAtom::Char('-')]
            })]
        );
    }

    // TODO ambiguous_character_range
    // TODO double_dash_at_start_of_bracket_expression
    // TODO double_dash_at_end_of_bracket_expression

    // TODO single_character_collating_symbol
    // TODO multi_character_collating_symbol
    // TODO single_character_equivalence_class
    // TODO multi_character_equivalence_class

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
