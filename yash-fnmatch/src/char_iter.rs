// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

use std::str::Chars;

/// Character appearing in patterns
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PatternChar {
    /// Normal pattern character
    ///
    /// `?`, `*`, `[` and `]` have special meaning in a pattern. Other `Normal`
    /// characters are the same as `Literal`.
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

/// TODO TBD
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

/// TODO TBD
#[must_use]
pub fn with_escape(pattern: &str) -> WithEscape {
    let chars = pattern.chars();
    WithEscape { chars }
}

/// TODO TBD
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

/// TODO TBD
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
