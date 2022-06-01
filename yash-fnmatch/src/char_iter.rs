// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

use std::str::Chars;

/// Type of characters appearing in patterns
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CharKind {
    /// Normal pattern character
    Normal,
    /// Quoted character, which is always regarded as a literal character
    Quoted,
}

use CharKind::*;

/// TODO TBD
#[derive(Clone, Debug)]
pub struct WithEscape<'a> {
    chars: Chars<'a>,
}

impl Iterator for WithEscape<'_> {
    type Item = (char, CharKind);
    fn next(&mut self) -> Option<(char, CharKind)> {
        match self.chars.next() {
            None => None,
            Some('\\') => self.chars.next().map(|c| (c, Quoted)),
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
    type Item = (char, CharKind);
    fn next(&mut self) -> Option<(char, CharKind)> {
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
        assert_eq!(v.as_slice(), [('a', Normal), ('b', Quoted), ('c', Normal)]);
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
