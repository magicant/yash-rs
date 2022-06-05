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
    Normal,

    /// Literal pattern character
    ///
    /// A literal character always matches itself.
    /// `?`, `*`, `[` and `]` lose special meaning when regarded literal.
    Literal,
}

use PatternChar::*;

/// TODO TBD
#[derive(Clone, Debug)]
pub struct WithEscape<'a> {
    chars: Chars<'a>,
}

impl Iterator for WithEscape<'_> {
    type Item = (char, PatternChar);
    fn next(&mut self) -> Option<(char, PatternChar)> {
        match self.chars.next() {
            None => None,
            Some('\\') => self.chars.next().map(|c| (c, Literal)),
            Some(c) => Some((c, Normal)),
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
    type Item = (char, PatternChar);
    fn next(&mut self) -> Option<(char, PatternChar)> {
        self.chars.next().map(|c| (c, Normal))
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
        assert_eq!(v.as_slice(), [('a', Normal), ('b', Literal), ('c', Normal)]);
    }

    #[test]
    fn without_escape_as_iterator() {
        let v: Vec<_> = without_escape(r"a\bc").collect();
        assert_eq!(
            v.as_slice(),
            [('a', Normal), ('\\', Normal), ('b', Normal), ('c', Normal)]
        );
    }
}
