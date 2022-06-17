// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! Abstract syntax tree for globbing patterns

mod parse;
mod regex;

use crate::PatternChar;
use std::ops::RangeInclusive;

/// Bracket expression component
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BracketAtom {
    /// Literal character
    Char(char),
    /// Collating symbol (e.g. `[.x.]`)
    CollatingSymbol(String),
    /// Equivalence Class (e.g. `[=x=]`)
    EquivalenceClass(String),
    /// Character class (e.g. `[:digit:]`)
    CharClass(String),
}

impl From<char> for BracketAtom {
    fn from(c: char) -> Self {
        BracketAtom::Char(c)
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

/// Abstract syntax tree for a whole pattern
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Ast {
    /// Content of the pattern
    pub atoms: Vec<Atom>,
}

impl Ast {
    /// Creates a pattern.
    #[inline]
    pub fn new<I>(pattern: I) -> Self
    where
        I: IntoIterator<Item = PatternChar>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        fn inner<I>(mut i: I) -> Ast
        where
            I: Iterator<Item = PatternChar> + Clone,
        {
            let mut atoms = Vec::new();
            while let Some((atom, j)) = Atom::parse(i) {
                atoms.push(atom);
                i = j;
            }
            Ast { atoms }
        }

        inner(pattern.into_iter())
    }

    /// Tests whether this pattern is completely literal.
    ///
    /// This function returns true iff all the atoms are `Atom::Char(_)`.
    ///
    /// ```
    /// # use yash_fnmatch::{ast::{Ast, Atom}, without_escape};
    /// assert!(Ast::new(without_escape("abc")).is_literal());
    /// assert!(!Ast::new(without_escape("a*c")).is_literal());
    /// ```
    #[must_use]
    pub fn is_literal(&self) -> bool {
        self.atoms.iter().all(|atom| matches!(atom, Atom::Char(_)))
    }

    /// Returns a matching string if this pattern is literal.
    ///
    /// If `self` is [literal](Self::is_literal), this function returns a string
    /// matched by the pattern. Otherwise, the result is `None`.
    ///
    /// ```
    /// # use yash_fnmatch::{ast::{Ast, Atom}, without_escape};
    /// assert_eq!(Ast::new(without_escape("abc")).to_literal(), Some("abc".to_string()));
    /// assert_eq!(Ast::new(without_escape("a*c")).to_literal(), None);
    /// ```
    #[must_use]
    pub fn to_literal(&self) -> Option<String> {
        self.atoms
            .iter()
            .map(|atom| match atom {
                Atom::Char(c) => Some(*c),
                _ => None,
            })
            .collect()
    }

    #[must_use]
    pub(crate) fn starts_with_literal_dot(&self) -> bool {
        self.atoms.first() == Some(&Atom::Char('.'))
    }
}
