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
use std::cell::RefCell;
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
    /// Rc::make_mut(&mut original.code).source = source;
    /// let source = Source::Alias{original, alias};
    /// assert_eq!(source.is_alias_for("foo"), true);
    /// assert_eq!(source.is_alias_for("bar"), true);
    /// assert_eq!(source.is_alias_for("baz"), false);
    /// ```
    pub fn is_alias_for(&self, name: &str) -> bool {
        if let Source::Alias { original, alias } = self {
            alias.name == name || original.code.source.is_alias_for(name)
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
    /// Content of the code, usually terminated by a newline.
    ///
    /// The value is contained in a `RefCell` so that more lines can be appended
    /// to the value as the parser reads input lines.
    pub value: RefCell<String>,

    /// Line number of the first line of the code. Counted from 1.
    pub start_line_number: NonZeroU64,

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

        let start_line_number = self.number;
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
        .to_string()
        .into();

        Some(Code {
            value,
            start_line_number,
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
            value: RefCell::new(String::new()),
            start_line_number: self.number,
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
    /// Code that contains the character.
    pub code: Rc<Code>,

    /// Character position in the code. Counted from 1.
    ///
    /// Characters are counted in the number of Unicode scalar values, not bytes.
    pub column: NonZeroU64,
}

impl Location {
    /// Creates a dummy location.
    ///
    /// The returned location has [unknown](Source::Unknown) source and the
    /// given source code value. The line and column numbers are 1.
    ///
    /// This function is mainly for use in testing.
    #[inline]
    pub fn dummy<S: Into<String>>(value: S) -> Location {
        fn with_line(value: String) -> Location {
            let value = RefCell::new(value);
            let number = NonZeroU64::new(1).unwrap();
            let code = Rc::new(Code {
                value,
                start_line_number: number,
                source: Source::Unknown,
            });
            Location {
                code,
                column: number,
            }
        }
        with_line(value.into())
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
        // We do need to collect the iterator to release the borrow.
        #[allow(clippy::needless_collect)]
        let chars = self.value.borrow().chars().collect::<Vec<char>>();
        chars.into_iter().zip(1u64..).map(move |(value, i)| {
            let column = NonZeroU64::new(i).unwrap();
            let location = Location {
                code: self.clone(),
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
        assert_eq!(*line.value.borrow(), "");
        assert_eq!(line.start_line_number.get(), 1);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn lines_one_line() {
        let mut l = lines("foo\n", Source::Unknown);

        let line = l.next().expect("first line should exist");
        assert_eq!(*line.value.borrow(), "foo\n");
        assert_eq!(line.start_line_number.get(), 1);
        assert_eq!(line.source, Source::Unknown);

        // No second line
        assert_eq!(l.next(), None);

        let line = l.next_or_empty();
        assert_eq!(*line.value.borrow(), "");
        assert_eq!(line.start_line_number.get(), 2);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn lines_three_lines() {
        let mut l = lines("foo\nbar\n\n", Source::Unknown);

        let line = l.next().expect("first line should exist");
        assert_eq!(*line.value.borrow(), "foo\n");
        assert_eq!(line.start_line_number.get(), 1);
        assert_eq!(line.source, Source::Unknown);

        let line = l.next().expect("second line should exist");
        assert_eq!(*line.value.borrow(), "bar\n");
        assert_eq!(line.start_line_number.get(), 2);
        assert_eq!(line.source, Source::Unknown);

        let line = l.next().expect("third line should exist");
        assert_eq!(*line.value.borrow(), "\n");
        assert_eq!(line.start_line_number.get(), 3);
        assert_eq!(line.source, Source::Unknown);

        // No more lines
        assert_eq!(l.next(), None);

        let line = l.next_or_empty();
        assert_eq!(*line.value.borrow(), "");
        assert_eq!(line.start_line_number.get(), 4);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn lines_without_trailing_newline() {
        let mut l = lines("one\ntwo", Source::Unknown);

        let line = l.next().expect("first line should exist");
        assert_eq!(*line.value.borrow(), "one\n");
        assert_eq!(line.start_line_number.get(), 1);
        assert_eq!(line.source, Source::Unknown);

        let line = l.next().expect("second line should exist");
        assert_eq!(*line.value.borrow(), "two");
        assert_eq!(line.start_line_number.get(), 2);
        assert_eq!(line.source, Source::Unknown);

        // No more lines
        assert_eq!(l.next(), None);
        // Lines is fused
        assert_eq!(l.next(), None);

        let line = l.next_or_empty();
        assert_eq!(*line.value.borrow(), "");
        assert_eq!(line.start_line_number.get(), 2);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn location_advance() {
        let code = Rc::new(lines("line\n", Source::Unknown).next().unwrap());
        let column = NonZeroU64::new(1).unwrap();
        let mut location = Location {
            code: code.clone(),
            column,
        };

        location.advance(1);
        assert_eq!(location.column.get(), 2);
        location.advance(2);
        assert_eq!(location.column.get(), 4);

        // The advance function does not check the line length.
        location.advance(5);
        assert_eq!(location.column.get(), 9);

        assert!(Rc::ptr_eq(&location.code, &code));
    }

    #[test]
    fn code_enumerate() {
        fn make_code(v: &str, n: u64) -> Rc<Code> {
            Rc::new(Code {
                value: v.to_string().into(),
                start_line_number: NonZeroU64::new(n).unwrap(),
                source: Source::Unknown,
            })
        }

        let empty = make_code("", 1);
        assert_eq!(empty.enumerate().next(), None);

        let code = make_code("foo", 2);
        let chars = code.enumerate().collect::<Vec<SourceChar>>();
        assert_eq!(chars.len(), 3);
        assert_eq!(chars[0].value, 'f');
        assert_eq!(chars[0].location.column.get(), 1);
        assert!(Rc::ptr_eq(&chars[0].location.code, &code));
        assert_eq!(chars[1].value, 'o');
        assert_eq!(chars[1].location.column.get(), 2);
        assert!(Rc::ptr_eq(&chars[1].location.code, &code));
        assert_eq!(chars[2].value, 'o');
        assert_eq!(chars[2].location.column.get(), 3);
        assert!(Rc::ptr_eq(&chars[2].location.code, &code));

        let code = make_code("hello", 4);
        let chars = code.enumerate().collect::<Vec<SourceChar>>();
        assert_eq!(chars.len(), 5);
        assert_eq!(chars[0].value, 'h');
        assert_eq!(chars[0].location.column.get(), 1);
        assert!(Rc::ptr_eq(&chars[0].location.code, &code));
        assert_eq!(chars[4].value, 'o');
        assert_eq!(chars[4].location.column.get(), 5);
        assert!(Rc::ptr_eq(&chars[4].location.code, &code));
    }
}
