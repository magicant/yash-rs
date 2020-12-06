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

use std::num::NonZeroU64;
use std::rc::Rc;

/// Origin of source code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// Source code of unknown origin.
    ///
    /// Normally you should not use this value, but it may be useful for quick debugging.
    Unknown,
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
