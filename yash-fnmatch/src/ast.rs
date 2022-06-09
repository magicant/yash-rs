// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! Abstract syntax tree for globbing patterns

use crate::Config;
use crate::Error;
use crate::PatternChar;

/// Bracket expression component
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BracketAtom {
    /// Literal character
    Char(char),
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
        if let Some(pc) = i.next() {
            if let Some(pc2) = i.next() {
                if pc2.char_value() == ']' {
                    return Ok(Some((
                        Bracket {
                            complement: false,
                            atoms: vec![BracketAtom::Char(pc.char_value())],
                        },
                        i,
                    )));
                }
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
    // TODO unmatched bracket []
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

    // TODO PatternChar Normal vs Literal
}
