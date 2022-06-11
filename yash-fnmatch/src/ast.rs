// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! Abstract syntax tree for globbing patterns

mod parse;
mod regex;

use crate::Error;
use crate::PatternChar;
use regex_syntax::ast::ClassAsciiKind;
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
    /// Character class (`[:digit:]`)
    CharClass(ClassAsciiKind),
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
    pub fn new<I>(pattern: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = PatternChar>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        let mut atoms = Vec::new();
        let mut i = pattern.into_iter();
        while let Some((atom, j)) = Atom::parse(i)? {
            atoms.push(atom);
            i = j;
        }
        Ok(Ast { atoms })
    }
}
