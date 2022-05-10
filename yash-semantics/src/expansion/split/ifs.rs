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

//! IFS parser

use std::borrow::Cow;

/// Type of characters that affect field splitting
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Class {
    /// Character that is not a separator
    NonIfs,
    /// Whitespace separator
    IfsWhitespace,
    /// Separator that is not whitespace
    IfsNonWhitespace,
}

/// Extracts a subsequence of the given string containing non-whitespace
/// characters only.
fn non_whitespaces(s: &str) -> Cow<str> {
    // Find a subsequence of `s` consisting of non-whitespaces
    let start = match s.find(|c: char| !c.is_whitespace()) {
        None => return Cow::Borrowed(&s[0..0]),
        Some(index) => index,
    };
    let s_start = &s[start..];
    let len = match s_start.find(char::is_whitespace) {
        None => return Cow::Borrowed(s_start),
        Some(index) => index,
    };
    let s_end = &s_start[len..];

    // Find another non-whitespace subsequence of `s`
    let start_2 = match s_end.find(|c: char| !c.is_whitespace()) {
        None => return Cow::Borrowed(&s_start[..len]),
        Some(index) => index,
    };
    let s_start_2 = &s_end[start_2..];

    // Create a new string containing non-whitespace separators only
    let mut non_whitespaces = String::with_capacity(len + s_start_2.len());
    non_whitespaces.push_str(&s_start[..len]);
    non_whitespaces.extend(s_start_2.chars().filter(|c| !c.is_whitespace()));
    Cow::Owned(non_whitespaces)
}

/// Collection of input field separator characters
#[derive(Clone, Debug, Eq)]
pub struct Ifs<'a> {
    chars: &'a str,
    non_whitespaces: Cow<'a, str>,
}

impl<'a> Ifs<'a> {
    /// Creates a new IFS consisting of the given separators.
    ///
    /// The argument is treated as a list of separator characters.
    pub fn new(chars: &'a str) -> Self {
        Ifs {
            chars,
            non_whitespaces: non_whitespaces(chars),
        }
    }

    /// Creates a new IFS containing no separators.
    pub fn empty() -> Self {
        Self::new("")
    }

    /// String containing the default separators.
    ///
    /// The default separators are a space, tab, and newline (`" \t\n"`).
    pub const DEFAULT: &'static str = " \t\n";
}

/// The default IFS contains a space, tab, and newline (`" \t\n"`).
impl Default for Ifs<'_> {
    fn default() -> Self {
        Self::new(Ifs::DEFAULT)
    }
}

/// The `==` operator compares [`self.chars()`](Self::chars) as a string.
///
/// That means two `Ifs` instances containing the same set of separators may not
/// compare equal if the original strings contained characters in different
/// orders.
impl PartialEq for Ifs<'_> {
    #[inline]
    fn eq(&self, other: &Ifs) -> bool {
        let chars_equal = self.chars == other.chars;
        if chars_equal {
            debug_assert_eq!(self.non_whitespaces, other.non_whitespaces);
        }
        chars_equal
    }
}

impl std::hash::Hash for Ifs<'_> {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        self.chars.hash(hasher)
    }
}

impl Ifs<'_> {
    /// Returns a string slice containing the separator characters.
    ///
    /// This function returns the original string slice used to create `*self`.
    #[inline]
    #[must_use]
    pub fn chars(&self) -> &str {
        self.chars
    }

    /// Returns a string slice containing the separator characters.
    ///
    /// This function returns a string slice cached in `*self`, which may be a
    /// substring of [`self.chars()`](Self::chars).
    #[must_use]
    pub fn non_whitespaces(&self) -> &str {
        &self.non_whitespaces
    }

    /// Tests if the given character is a separator contained in this IFS.
    ///
    /// ```
    /// # use yash_semantics::expansion::split::Ifs;
    /// let ifs = Ifs::new(" a");
    /// assert!(ifs.is_ifs(' '));
    /// assert!(ifs.is_ifs('a'));
    /// assert!(!ifs.is_ifs('b'));
    /// ```
    #[inline]
    #[must_use]
    pub fn is_ifs(&self, c: char) -> bool {
        self.chars.contains(c)
    }

    /// Tests if the given character is an IFS-non-whitespace.
    ///
    /// This function returns true iff the character is included in
    /// [`self.chars()`](Self::chars) and is not whitespace.
    ///
    /// ```
    /// # use yash_semantics::expansion::split::Ifs;
    /// let ifs = Ifs::new(" a");
    /// assert!(ifs.is_ifs_non_whitespace('a'));
    ///
    /// // The space character is included in the IFS, but is whitespace.
    /// assert!(ifs.is_ifs(' '));
    ///
    /// // The character 'b' is not whitespace, but not included in the IFS.
    /// assert!(!ifs.is_ifs('b'));
    /// ```
    #[inline]
    #[must_use]
    pub fn is_ifs_non_whitespace(&self, c: char) -> bool {
        self.non_whitespaces.contains(c)
    }

    /// Returns the type of the character.
    #[must_use]
    pub fn classify(&self, c: char) -> Class {
        if self.is_ifs(c) {
            if self.is_ifs_non_whitespace(c) {
                Class::IfsNonWhitespace
            } else {
                Class::IfsWhitespace
            }
        } else {
            Class::NonIfs
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_non_ifs(ifs: &Ifs, c: char) {
        assert!(!ifs.is_ifs(c), "{c:?} not to be separator");
        assert!(!ifs.is_ifs_non_whitespace(c), "{c:?} not to be separator");
        assert_eq!(ifs.classify(c), Class::NonIfs, "{c:?} not to be separator");
    }

    fn assert_ifs_whitespace(ifs: &Ifs, c: char) {
        assert!(ifs.is_ifs(c), "{c:?} to be IFS-whitespace");
        assert!(!ifs.is_ifs_non_whitespace(c), "{c:?} to be IFS-whitespace");
        assert_eq!(
            ifs.classify(c),
            Class::IfsWhitespace,
            "{c:?} to be IFS-whitespace"
        );
    }

    fn assert_ifs_non_whitespace(ifs: &Ifs, c: char) {
        assert!(ifs.is_ifs(c), "{c:?} to be IFS-non-whitespace");
        assert!(
            ifs.is_ifs_non_whitespace(c),
            "{c:?} to be IFS-non-whitespace"
        );
        assert_eq!(
            ifs.classify(c),
            Class::IfsNonWhitespace,
            "{c:?} to be IFS-non-whitespace"
        );
    }

    #[test]
    fn empty_ifs() {
        let ifs = Ifs::empty();
        assert_non_ifs(&ifs, ' ');
        assert_non_ifs(&ifs, '\t');
        assert_non_ifs(&ifs, '\n');
        assert_non_ifs(&ifs, '\r');
        assert_non_ifs(&ifs, 'a');
        assert_non_ifs(&ifs, '-');
    }

    #[test]
    fn default_ifs() {
        let ifs = Ifs::default();
        assert_ifs_whitespace(&ifs, ' ');
        assert_ifs_whitespace(&ifs, '\t');
        assert_ifs_whitespace(&ifs, '\n');
        assert_non_ifs(&ifs, '\r');
        assert_non_ifs(&ifs, 'a');
        assert_non_ifs(&ifs, '-');
    }

    #[test]
    fn non_default_ifs_whitespaces() {
        let ifs = Ifs::new(" \r\u{A0}\u{3000}");
        assert_ifs_whitespace(&ifs, ' ');
        assert_ifs_whitespace(&ifs, '\r');
        assert_ifs_whitespace(&ifs, '\u{A0}');
        assert_ifs_whitespace(&ifs, '\u{3000}');
        assert_non_ifs(&ifs, '\t');
        assert_non_ifs(&ifs, '\n');
    }

    #[test]
    fn ifs_non_whitespaces() {
        let ifs = Ifs::new("a-");
        assert_non_ifs(&ifs, ' ');
        assert_non_ifs(&ifs, '\t');
        assert_non_ifs(&ifs, '\n');
        assert_non_ifs(&ifs, '\r');
        assert_ifs_non_whitespace(&ifs, 'a');
        assert_ifs_non_whitespace(&ifs, '-');
    }

    #[test]
    fn ifs_whitespaces_and_non_whitespaces() {
        let ifs = Ifs::new(" a");
        assert_ifs_whitespace(&ifs, ' ');
        assert_non_ifs(&ifs, '\t');
        assert_non_ifs(&ifs, '\n');
        assert_non_ifs(&ifs, '\r');
        assert_ifs_non_whitespace(&ifs, 'a');
        assert_non_ifs(&ifs, '-');
    }

    #[test]
    fn ifs_non_whitespace_and_whitespace() {
        let ifs = Ifs::new("a ");
        assert_ifs_whitespace(&ifs, ' ');
        assert_non_ifs(&ifs, '\t');
        assert_non_ifs(&ifs, '\n');
        assert_non_ifs(&ifs, '\r');
        assert_ifs_non_whitespace(&ifs, 'a');
        assert_non_ifs(&ifs, '-');
    }

    #[test]
    fn ifs_whitespace_and_non_whitespace_and_whitespace() {
        let ifs = Ifs::new(" a\t");
        assert_ifs_whitespace(&ifs, ' ');
        assert_ifs_whitespace(&ifs, '\t');
        assert_non_ifs(&ifs, '\n');
        assert_non_ifs(&ifs, '\r');
        assert_ifs_non_whitespace(&ifs, 'a');
        assert_non_ifs(&ifs, '-');
    }

    #[test]
    fn ifs_non_whitespace_and_whitespace_and_non_whitespace() {
        let ifs = Ifs::new("a -");
        assert_ifs_whitespace(&ifs, ' ');
        assert_non_ifs(&ifs, '\t');
        assert_non_ifs(&ifs, '\n');
        assert_non_ifs(&ifs, '\r');
        assert_ifs_non_whitespace(&ifs, 'a');
        assert_ifs_non_whitespace(&ifs, '-');
    }

    #[test]
    fn more_alternating_whitespaces_and_non_whitespaces() {
        let ifs = Ifs::new(" a b c d ");
        assert_ifs_whitespace(&ifs, ' ');
        assert_ifs_non_whitespace(&ifs, 'a');
        assert_ifs_non_whitespace(&ifs, 'b');
        assert_ifs_non_whitespace(&ifs, 'c');
        assert_ifs_non_whitespace(&ifs, 'd');
        assert_non_ifs(&ifs, 'e');
    }

    #[test]
    fn eq() {
        assert_eq!(Ifs::empty(), Ifs::empty());
        assert_eq!(Ifs::default(), Ifs::default());
        assert_eq!(Ifs::new(" a-"), Ifs::new(" a-"));
        assert_ne!(Ifs::empty(), Ifs::default());
        assert_ne!(Ifs::default(), Ifs::new(" a-"));
        assert_ne!(Ifs::new(" a-"), Ifs::new(" b-"));
    }
}
