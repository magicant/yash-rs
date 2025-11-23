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

//! Shell script source code
//!
//! This module re-exports source code related items from the
//! [`yash-env`](yash_env) crate. It also defines the [`SourceChar`] struct
//! representing a character with source description.

use std::rc::Rc;

#[doc(no_inline)]
pub use yash_env::source::*;

/// Creates an iterator of [source char](SourceChar)s from a string.
///
/// `index_offset` will be the index of the first source char's location.
/// For each succeeding char, the index will be incremented by one.
///
/// ```
/// # use yash_syntax::source::{Code, Source, source_chars};
/// # use std::cell::RefCell;
/// # use std::num::NonZeroU64;
/// # use std::rc::Rc;
/// let s = "abc";
/// let code = Rc::new(Code {
///     value: RefCell::new(s.to_string()),
///     start_line_number: NonZeroU64::new(1).unwrap(),
///     source: Rc::new(Source::Unknown),
/// });
/// let chars: Vec<_> = source_chars(s, &code, 10).collect();
/// assert_eq!(chars[0].value, 'a');
/// assert_eq!(chars[0].location.code, code);
/// assert_eq!(chars[0].location.range, 10..11);
/// assert_eq!(chars[1].value, 'b');
/// assert_eq!(chars[1].location.code, code);
/// assert_eq!(chars[1].location.range, 11..12);
/// ```
pub fn source_chars<'a>(
    s: &'a str,
    code: &'a Rc<Code>,
    index_offset: usize,
) -> impl Iterator<Item = SourceChar> + 'a {
    s.chars().enumerate().map(move |(i, value)| SourceChar {
        value,
        location: Location {
            code: Rc::clone(code),
            range: index_offset + i..index_offset + i + 1,
        },
    })
}

/// Character with source description
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceChar {
    /// Character value
    pub value: char,
    /// Location of this character in source code
    pub location: Location,
}
