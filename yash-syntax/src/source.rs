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
//! This module contains items representing information about the source code
//! from which ASTs originate. [`Source`] identifies the origin of source code
//! fragments contained in [`Code`]. A [`Location`] specifies a particular
//! character in a `Code` instance. You can use the [`pretty`] submodule to
//! format messages describing source code locations.

pub mod pretty;

use crate::alias::Alias;
use std::cell::RefCell;
use std::num::NonZeroU64;
use std::ops::Range;
use std::rc::Rc;

/// Origin of source code
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Source {
    /// Source code of unknown origin
    ///
    /// Normally you should not use this value, but it may be useful for quick debugging.
    Unknown,

    /// Standard input
    Stdin,

    /// Command string specified with the `-c` option on the shell startup
    CommandString,

    /// File specified on the shell startup
    CommandFile { path: String },

    /// Alias substitution
    ///
    /// This applies to a code fragment that replaced another as a result of alias substitution.
    Alias {
        /// Position of the original word that was replaced
        original: Location,
        /// Definition of the alias that was substituted
        alias: Rc<Alias>,
    },

    /// Command substitution
    CommandSubst { original: Location },

    /// Arithmetic expansion
    Arith { original: Location },

    /// Command string executed by the `eval` built-in
    Eval { original: Location },

    /// File executed by the `.` (`source`) built-in
    DotScript {
        /// Pathname of the file
        name: String,
        /// Location of the simple command that invoked the `.` built-in
        origin: Location,
    },

    /// Trap command
    Trap {
        /// Trap condition name, typically the signal name
        condition: String,
        /// Location of the simple command that has set this trap command
        origin: Location,
    },

    /// Value of a variable
    VariableValue {
        /// Variable name
        name: String,
    },

    /// File executed during shell startup
    InitFile { path: String },

    /// Other source
    Other {
        /// Label that describes the source
        label: String,
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
    /// let alias = std::rc::Rc::new(yash_syntax::alias::Alias {
    ///     name: "foo".to_string(),
    ///     replacement: "".to_string(),
    ///     global: false,
    ///     origin: original.clone()
    /// });
    /// let source = Source::Alias { original, alias };
    /// assert_eq!(source.is_alias_for("foo"), true);
    /// assert_eq!(source.is_alias_for("bar"), false);
    /// ```
    ///
    /// ```
    /// // `is_alias_for` checks aliases recursively.
    /// # use std::rc::Rc;
    /// # use yash_syntax::source::*;
    /// let original = Location::dummy("");
    /// let alias = Rc::new(yash_syntax::alias::Alias {
    ///     name: "foo".to_string(),
    ///     replacement: "".to_string(),
    ///     global: false,
    ///     origin: original.clone(),
    /// });
    /// let source = Source::Alias { original, alias };
    /// let alias = Rc::new(yash_syntax::alias::Alias {
    ///     name: "bar".to_string(),
    ///     replacement: "".to_string(),
    ///     global: false,
    ///     origin: Location::dummy(""),
    /// });
    /// let mut original = Location::dummy("");
    /// Rc::make_mut(&mut original.code).source = Rc::new(source);
    /// let source = Source::Alias { original, alias };
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
            CommandString => "<command_string>",
            CommandFile { path } => path,
            Alias { .. } => "<alias>",
            CommandSubst { .. } => "<command_substitution>",
            Arith { .. } => "<arithmetic_expansion>",
            Eval { .. } => "<eval>",
            DotScript { name, .. } => name,
            Trap { condition, .. } => condition,
            VariableValue { name } => name,
            InitFile { path } => path,
            Other { label } => label,
        }
    }
}

/// Source code fragment
///
/// An instance of `Code` contains a block of the source code that was parsed to
/// produce an AST.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Code {
    /// Content of the code, usually terminated by a newline
    ///
    /// The value is contained in a `RefCell` so that more lines can be appended
    /// to the value as the parser reads input lines. It is not intended to be
    /// mutably borrowed for other purposes.
    pub value: RefCell<String>,

    /// Line number of the first line of the code. Counted from 1.
    pub start_line_number: NonZeroU64,

    /// Origin of this code
    pub source: Rc<Source>,
}

impl Code {
    /// Computes the line number of the character at the given index.
    ///
    /// The index should be between 0 and `self.value.borrow().chars().count()`.
    /// The return value is `self.start_line_number` plus the number of newlines
    /// in `self.value` up to the character at `char_index`. If `char_index` is
    /// out of bounds, the return value is for the last character.
    ///
    /// This function will panic if `self.value` has been mutually borrowed.
    #[must_use]
    pub fn line_number(&self, char_index: usize) -> NonZeroU64 {
        let newlines = self
            .value
            .borrow()
            .chars()
            .take(char_index)
            .filter(|c| *c == '\n')
            .count()
            .try_into()
            .unwrap_or(u64::MAX);
        self.start_line_number.saturating_add(newlines)
    }
}

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

/// Position of source code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    /// Code that contains the character.
    pub code: Rc<Code>,

    /// Character position in the code, counted from 0.
    ///
    /// Characters are counted in the number of Unicode scalar values, not
    /// bytes. That means the index should be between 0 and
    /// `code.value.borrow().chars().count()`.
    pub range: Range<usize>,
}

impl Location {
    /// Creates a dummy location.
    ///
    /// The returned location has [unknown](Source::Unknown) source and the
    /// given source code value. The `start_line_number` will be 1.
    /// The location ranges over the whole code.
    ///
    /// This function is mainly for use in testing.
    #[inline]
    pub fn dummy<S: Into<String>>(value: S) -> Location {
        fn with_line(value: String) -> Location {
            let range = 0..value.chars().count();
            let code = Rc::new(Code {
                value: RefCell::new(value),
                start_line_number: NonZeroU64::new(1).unwrap(),
                source: Rc::new(Source::Unknown),
            });
            Location { code, range }
        }
        with_line(value.into())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_number() {
        let code = Code {
            value: RefCell::new("a\nbc\nd".to_string()),
            start_line_number: NonZeroU64::new(1).unwrap(),
            source: Rc::new(Source::Unknown),
        };
        assert_eq!(code.line_number(0).get(), 1);
        assert_eq!(code.line_number(1).get(), 1);
        assert_eq!(code.line_number(2).get(), 2);
        assert_eq!(code.line_number(3).get(), 2);
        assert_eq!(code.line_number(4).get(), 2);
        assert_eq!(code.line_number(5).get(), 3);
        assert_eq!(code.line_number(6).get(), 3);
        assert_eq!(code.line_number(7).get(), 3);
        assert_eq!(code.line_number(usize::MAX).get(), 3);

        let code = Code {
            start_line_number: NonZeroU64::new(3).unwrap(),
            ..code
        };
        assert_eq!(code.line_number(0).get(), 3);
        assert_eq!(code.line_number(1).get(), 3);
        assert_eq!(code.line_number(2).get(), 4);
        assert_eq!(code.line_number(3).get(), 4);
        assert_eq!(code.line_number(4).get(), 4);
        assert_eq!(code.line_number(5).get(), 5);
        assert_eq!(code.line_number(6).get(), 5);
        assert_eq!(code.line_number(7).get(), 5);
        assert_eq!(code.line_number(usize::MAX).get(), 5);
    }
}
