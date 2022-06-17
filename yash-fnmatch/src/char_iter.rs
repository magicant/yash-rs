// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

#[cfg(doc)]
use crate::Pattern;
use std::str::Chars;

/// Character appearing in patterns
///
/// The [`with_escape`] and [`without_escape`] functions return an iterator that
/// yields pattern characters.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PatternChar {
    /// Normal pattern character
    ///
    /// `?`, `*`, `[` and `]` have special meaning when used in a pattern. Other
    /// `Normal` characters are the same as `Literal`.
    Normal(char),

    /// Literal pattern character
    ///
    /// A literal character always matches itself.
    /// `?`, `*`, `[` and `]` lose special meaning when regarded literal.
    Literal(char),
}

use PatternChar::*;

impl PatternChar {
    /// Returns the character value.
    #[inline]
    #[must_use]
    pub const fn char_value(self) -> char {
        match self {
            Normal(c) => c,
            Literal(c) => c,
        }
    }
}

/// Iterator returned by [`with_escape`]
#[derive(Clone, Debug)]
pub struct WithEscape<'a> {
    chars: Chars<'a>,
}

impl Iterator for WithEscape<'_> {
    type Item = PatternChar;
    fn next(&mut self) -> Option<PatternChar> {
        match self.chars.next() {
            None => None,
            Some('\\') => self.chars.next().map(Literal),
            Some(c) => Some(Normal(c)),
        }
    }
}

/// Adapts an escaped string for input to a parser.
///
/// This function returns an iterator suitable to be passed to
/// [`Pattern::parse`] and other parsing functions.
/// Backslashes in the string act as escape characters.
///
/// ```
/// # use yash_fnmatch::{ast::{Ast, Atom}, with_escape};
/// // The backslash escapes the asterisk, which is parsed as a literal
/// // character rather than a wildcard pattern.
/// let ast = Ast::new(with_escape(r"\*"));
/// assert_eq!(ast.atoms, [Atom::Char('*')]);
/// ```
///
/// Compare [`without_escape`], which ignores backslash escapes.
#[must_use]
pub fn with_escape(pattern: &str) -> WithEscape {
    let chars = pattern.chars();
    WithEscape { chars }
}

/// Iterator returned by [`without_escape`]
#[derive(Clone, Debug)]
pub struct WithoutEscape<'a> {
    chars: Chars<'a>,
}

impl Iterator for WithoutEscape<'_> {
    type Item = PatternChar;
    fn next(&mut self) -> Option<PatternChar> {
        self.chars.next().map(Normal)
    }
}

/// Adapts a literal string for input to a parser.
///
/// This function returns an iterator suitable to be passed to
/// [`Pattern::parse`] and other parsing functions.
/// Backslashes in the string do not act as escape characters.
///
/// ```
/// # use yash_fnmatch::{ast::{Ast, Atom}, without_escape};
/// // The backslash just matches a backslash itself.
/// // The asterisk works as a wildcard pattern.
/// let ast = Ast::new(without_escape(r"\*"));
/// assert_eq!(ast.atoms, [Atom::Char('\\'), Atom::AnyString]);
/// ```
///
/// Compare [`with_escape`], which handles backslash escapes.
#[must_use]
pub fn without_escape(pattern: &str) -> WithoutEscape {
    let chars = pattern.chars();
    WithoutEscape { chars }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_escape_as_iterator() {
        let v: Vec<_> = with_escape(r"a\bc").collect();
        assert_eq!(v.as_slice(), [Normal('a'), Literal('b'), Normal('c')]);
    }

    #[test]
    fn without_escape_as_iterator() {
        let v: Vec<_> = without_escape(r"a\bc").collect();
        assert_eq!(
            v.as_slice(),
            [Normal('a'), Normal('\\'), Normal('b'), Normal('c')]
        );
    }
}
