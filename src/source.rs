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

use crate::alias::Alias;
use std::num::NonZeroU64;
use std::rc::Rc;

/// Origin of source code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// Source code of unknown origin.
    ///
    /// Normally you should not use this value, but it may be useful for quick debugging.
    Unknown,
    /// Alias substitution.
    ///
    /// This applies to a code fragment that replaced another as a result of alias substitution.
    ///
    /// `original` is the location of the original word that was replaced.
    Alias {
        original: Location,
        alias: Rc<Alias>,
    },
    // TODO More Source types
}

/// Line in source code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Line {
    /// Content of the line, usually including a trailing newline.
    ///
    /// A line must be terminated by a newline character (unless the source code lacks a
    /// newline in the last line). Newlines must not appear in any other part of the line.
    pub value: String,
    /// Line number. Counted from 1.
    pub number: NonZeroU64,
    /// Source code containing this line.
    pub source: Source,
}

/// Iterator created by [lines].
pub struct Lines<'a> {
    source: Source,
    code: &'a str,
    number: u64,
}

impl<'a> Iterator for Lines<'a> {
    type Item = Line;

    fn next(&mut self) -> Option<Line> {
        if self.code.is_empty() {
            return None;
        }

        self.number += 1;
        let number = NonZeroU64::new(self.number).unwrap();
        let source = self.source.clone();

        let value = match self.code.find('\n') {
            None => {
                let value_range = self.code;
                self.code = &self.code[self.code.len()..];
                value_range
            }
            Some(mut i) => {
                i += 1;
                let value_range = &self.code[..i];
                self.code = &self.code[i..];
                value_range
            }
        }
        .to_string();

        Some(Line {
            value,
            number,
            source,
        })
    }
}

/// Converts a source code string into an iterator of [Line]s.
pub fn lines(source: Source, code: &str) -> Lines<'_> {
    Lines {
        source,
        code,
        number: 0,
    }
}

/// Position of a character in source code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    /// Line that contains the character.
    pub line: Rc<Line>,

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
    pub fn dummy(line: String) -> Location {
        let number = NonZeroU64::new(1).unwrap();
        let line = Rc::new(Line {
            value: line,
            number,
            source: Source::Unknown,
        });
        Location {
            line,
            column: number,
        }
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

impl Line {
    /// Creates an iterator of `SourceChar`.
    ///
    /// The character columns are counted from 1.
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
        assert_eq!(lines(Source::Unknown, "").next(), None);
    }

    #[test]
    fn lines_one_line() {
        let mut l = lines(Source::Unknown, "foo\n");

        let line = l.next().expect("first line should exist");
        assert_eq!(&line.value, "foo\n");
        assert_eq!(line.number.get(), 1);
        assert_eq!(line.source, Source::Unknown);

        // No second line
        assert_eq!(l.next(), None);
    }

    #[test]
    fn lines_three_lines() {
        let mut l = lines(Source::Unknown, "foo\nbar\n\n");

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
    }

    #[test]
    fn lines_without_trailing_newline() {
        let mut l = lines(Source::Unknown, "one\ntwo");

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
    }

    #[test]
    fn location_advance() {
        let line = Rc::new(lines(Source::Unknown, "line\n").next().unwrap());
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
    fn line_enumerate() {
        fn make_line(v: &str, n: u64) -> Rc<Line> {
            Rc::new(Line {
                value: v.to_string(),
                number: NonZeroU64::new(n).unwrap(),
                source: Source::Unknown,
            })
        }

        let empty = make_line("", 1);
        assert_eq!(empty.enumerate().next(), None);

        let line = make_line("foo", 2);
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

        let line = make_line("hello", 4);
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
