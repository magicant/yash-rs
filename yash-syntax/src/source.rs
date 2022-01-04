// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2020 WATANABE Yuki
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

//! Source code that is passed to the parser.
//!
//! TODO Elaborate

pub mod pretty;

use crate::alias::Alias;
use std::iter::FusedIterator;
use std::num::NonZeroU64;
use std::rc::Rc;

/// Origin of source code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// Source code of unknown origin.
    ///
    /// Normally you should not use this value, but it may be useful for quick debugging.
    Unknown,

    /// Standard input.
    Stdin,

    /// Alias substitution.
    ///
    /// This applies to a code fragment that replaced another as a result of alias substitution.
    ///
    /// `original` is the location of the original word that was replaced.
    Alias {
        original: Location,
        alias: Rc<Alias>,
    },

    /// Command substitution.
    CommandSubst { original: Location },

    /// Trap command.
    Trap {
        /// Trap condition name, typically the signal name.
        condition: String,
        /// Location of the simple command that has set this trap command.
        origin: Location,
    },
    // TODO More Source types
}

impl Source {
    /// Tests if this source is alias substitution for the given name.
    ///
    /// Returns true if `self` is `Source::Alias` with the `name` or such an
    /// original, recursively.
    ///
    /// ```
    /// // `is_alias_for` returns false for sources other than an Alias
    /// # use yash_syntax::source::Source;
    /// assert_eq!(Source::Unknown.is_alias_for("foo"), false);
    /// ```
    ///
    /// ```
    /// // `is_alias_for` returns true if the names match
    /// # use yash_syntax::source::*;
    /// let original = Location::dummy("");
    /// let alias = std::rc::Rc::new(yash_syntax::alias::Alias{
    ///     name: "foo".to_string(),
    ///     replacement: "".to_string(),
    ///     global: false,
    ///     origin: original.clone()
    /// });
    /// let source = Source::Alias{original, alias};
    /// assert_eq!(source.is_alias_for("foo"), true);
    /// assert_eq!(source.is_alias_for("bar"), false);
    /// ```
    ///
    /// ```
    /// // `is_alias_for` checks aliases recursively.
    /// # use std::rc::Rc;
    /// # use yash_syntax::source::*;
    /// let mut original = Location::dummy("");
    /// let alias = Rc::new(yash_syntax::alias::Alias{
    ///     name: "foo".to_string(),
    ///     replacement: "".to_string(),
    ///     global: false,
    ///     origin: original.clone()
    /// });
    /// let source = Source::Alias{original: original.clone(), alias};
    /// let alias = Rc::new(yash_syntax::alias::Alias{
    ///     name: "bar".to_string(),
    ///     replacement: "".to_string(),
    ///     global: false,
    ///     origin: original.clone()
    /// });
    /// Rc::make_mut(&mut original.line).source = source;
    /// let source = Source::Alias{original, alias};
    /// assert_eq!(source.is_alias_for("foo"), true);
    /// assert_eq!(source.is_alias_for("bar"), true);
    /// assert_eq!(source.is_alias_for("baz"), false);
    /// ```
    pub fn is_alias_for(&self, name: &str) -> bool {
        if let Source::Alias { original, alias } = self {
            alias.name == name || original.line.source.is_alias_for(name)
        } else {
            false
        }
    }

    /// Returns a label that describes the source.
    pub fn label(&self) -> &str {
        use Source::*;
        match self {
            Unknown => "<?>",
            Stdin => "<stdin>",
            Alias { .. } => "<alias>",
            CommandSubst { .. } => "<command_substitution>",
            Trap { condition, .. } => condition,
        }
    }
}

/// Source code fragment
///
/// TODO Elaborate
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Code {
    /// Content of the code, usually including a trailing newline.
    pub value: String,
    /// Line number. Counted from 1.
    pub number: NonZeroU64,
    /// Source of this code.
    pub source: Source,
}

/// Iterator created by [lines].
pub struct Lines<'a> {
    source: Source,
    code: &'a str,
    number: NonZeroU64,
}

impl<'a> Iterator for Lines<'a> {
    type Item = Code;

    fn next(&mut self) -> Option<Code> {
        if self.code.is_empty() {
            return None;
        }

        let number = self.number;
        let source = self.source.clone();

        let value = match self.code.find('\n') {
            None => {
                let value_range = self.code;
                self.code = &self.code[self.code.len()..];
                value_range
            }
            Some(mut i) => {
                i += 1;
                // TODO self.number = self.number.saturating_add(1);
                self.number =
                    unsafe { NonZeroU64::new_unchecked(self.number.get().saturating_add(1)) };
                let value_range = &self.code[..i];
                self.code = &self.code[i..];
                value_range
            }
        }
        .to_string();

        Some(Code {
            value,
            number,
            source,
        })
    }
}

impl FusedIterator for Lines<'_> {}

impl Lines<'_> {
    /// Like `next`, but returns an empty line if the end of input has been
    /// reached.
    pub fn next_or_empty(&mut self) -> Code {
        self.next().unwrap_or_else(|| Code {
            value: String::new(),
            number: self.number,
            source: self.source.clone(),
        })
    }
}

/// Creates an iterator of lines.
///
/// TODO Elaborate: Each `Code` yielded by the iterator contains a single line of code.
pub fn lines(code: &str, source: Source) -> Lines<'_> {
    Lines {
        source,
        code,
        number: NonZeroU64::new(1).unwrap(),
    }
}

/// Position of a character in source code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    /// Line that contains the character.
    pub line: Rc<Code>,

    /// Character position in the line. Counted from 1.
    ///
    /// Characters are counted in the number of Unicode scalar values, not bytes.
    pub column: NonZeroU64,
}

impl Location {
    /// Creates a dummy location.
    ///
    /// The returned location has [unknown](Source::Unknown) source and the given line value. The
    /// line and column numbers are 1.
    ///
    /// This function is mainly for use in testing.
    #[inline]
    pub fn dummy<S: Into<String>>(line: S) -> Location {
        fn with_line(line: String) -> Location {
            let number = NonZeroU64::new(1).unwrap();
            let line = Rc::new(Code {
                value: line,
                number,
                source: Source::Unknown,
            });
            Location {
                line,
                column: number,
            }
        }
        with_line(line.into())
    }

    /// Increases the column number
    pub fn advance(&mut self, count: u64) {
        let column = self.column.get().checked_add(count).unwrap();
        self.column = NonZeroU64::new(column).unwrap();
    }
}

/// Character with source description.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceChar {
    /// Character value.
    pub value: char,
    /// Location of this character in source code.
    pub location: Location,
}

impl Code {
    /// Creates an iterator of `SourceChar`.
    ///
    /// The character columns are counted from 1.
    #[allow(clippy::needless_lifetimes)] // This lifetime is actually needed.
    pub fn enumerate<'a>(self: &'a Rc<Self>) -> impl Iterator<Item = SourceChar> + 'a {
        self.value.chars().zip(1u64..).map(move |(value, i)| {
            let column = NonZeroU64::new(i).unwrap();
            let location = Location {
                line: self.clone(),
                column,
            };
            SourceChar { value, location }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_empty() {
        let mut l = lines("", Source::Unknown);
        assert_eq!(l.next(), None);

        let line = l.next_or_empty();
        assert_eq!(&line.value, "");
        assert_eq!(line.number.get(), 1);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn lines_one_line() {
        let mut l = lines("foo\n", Source::Unknown);

        let line = l.next().expect("first line should exist");
        assert_eq!(&line.value, "foo\n");
        assert_eq!(line.number.get(), 1);
        assert_eq!(line.source, Source::Unknown);

        // No second line
        assert_eq!(l.next(), None);

        let line = l.next_or_empty();
        assert_eq!(&line.value, "");
        assert_eq!(line.number.get(), 2);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn lines_three_lines() {
        let mut l = lines("foo\nbar\n\n", Source::Unknown);

        let line = l.next().expect("first line should exist");
        assert_eq!(&line.value, "foo\n");
        assert_eq!(line.number.get(), 1);
        assert_eq!(line.source, Source::Unknown);

        let line = l.next().expect("second line should exist");
        assert_eq!(&line.value, "bar\n");
        assert_eq!(line.number.get(), 2);
        assert_eq!(line.source, Source::Unknown);

        let line = l.next().expect("third line should exist");
        assert_eq!(&line.value, "\n");
        assert_eq!(line.number.get(), 3);
        assert_eq!(line.source, Source::Unknown);

        // No more lines
        assert_eq!(l.next(), None);

        let line = l.next_or_empty();
        assert_eq!(&line.value, "");
        assert_eq!(line.number.get(), 4);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn lines_without_trailing_newline() {
        let mut l = lines("one\ntwo", Source::Unknown);

        let line = l.next().expect("first line should exist");
        assert_eq!(&line.value, "one\n");
        assert_eq!(line.number.get(), 1);
        assert_eq!(line.source, Source::Unknown);

        let line = l.next().expect("second line should exist");
        assert_eq!(&line.value, "two");
        assert_eq!(line.number.get(), 2);
        assert_eq!(line.source, Source::Unknown);

        // No more lines
        assert_eq!(l.next(), None);
        // Lines is fused
        assert_eq!(l.next(), None);

        let line = l.next_or_empty();
        assert_eq!(&line.value, "");
        assert_eq!(line.number.get(), 2);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn location_advance() {
        let line = Rc::new(lines("line\n", Source::Unknown).next().unwrap());
        let column = NonZeroU64::new(1).unwrap();
        let mut location = Location {
            line: line.clone(),
            column,
        };

        location.advance(1);
        assert_eq!(location.column.get(), 2);
        location.advance(2);
        assert_eq!(location.column.get(), 4);

        // The advance function does not check the line length.
        location.advance(5);
        assert_eq!(location.column.get(), 9);

        assert!(Rc::ptr_eq(&location.line, &line));
    }

    #[test]
    fn code_enumerate() {
        fn make_code(v: &str, n: u64) -> Rc<Code> {
            Rc::new(Code {
                value: v.to_string(),
                number: NonZeroU64::new(n).unwrap(),
                source: Source::Unknown,
            })
        }

        let empty = make_code("", 1);
        assert_eq!(empty.enumerate().next(), None);

        let line = make_code("foo", 2);
        let chars = line.enumerate().collect::<Vec<SourceChar>>();
        assert_eq!(chars.len(), 3);
        assert_eq!(chars[0].value, 'f');
        assert_eq!(chars[0].location.column.get(), 1);
        assert!(Rc::ptr_eq(&chars[0].location.line, &line));
        assert_eq!(chars[1].value, 'o');
        assert_eq!(chars[1].location.column.get(), 2);
        assert!(Rc::ptr_eq(&chars[1].location.line, &line));
        assert_eq!(chars[2].value, 'o');
        assert_eq!(chars[2].location.column.get(), 3);
        assert!(Rc::ptr_eq(&chars[2].location.line, &line));

        let line = make_code("hello", 4);
        let chars = line.enumerate().collect::<Vec<SourceChar>>();
        assert_eq!(chars.len(), 5);
        assert_eq!(chars[0].value, 'h');
        assert_eq!(chars[0].location.column.get(), 1);
        assert!(Rc::ptr_eq(&chars[0].location.line, &line));
        assert_eq!(chars[4].value, 'o');
        assert_eq!(chars[4].location.column.get(), 5);
        assert!(Rc::ptr_eq(&chars[4].location.line, &line));
    }
}
