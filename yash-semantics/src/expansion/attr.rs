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

//! Intermediate expansion results
//!
//! This module defines some types that represent intermediate results of
//! the expansion.
//!
//! An [`AttrChar`] is a character with attributes describing how the character
//! was derived in the initial expansion. The attributes affect the behavior of
//! later steps of the expansion. An [`AttrField`] is a string of `AttrChar`s
//! associated with the location of the originating word.

use yash_env::semantics::Field;
use yash_syntax::source::Location;

/// Category of syntactic elements from which expansion originates.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Origin {
    /// The character appeared literally in the original word.
    Literal,

    /// The character originates from a tilde expansion or sequencing brace
    /// expansion.
    ///
    /// This kind of character is treated literally in the pathname expansion.
    HardExpansion,

    /// The character originates from a parameter expansion, command
    /// substitution, or arithmetic expansion.
    ///
    /// This kind of character is subject to field splitting where applicable.
    SoftExpansion,
}

/// Character with attributes describing its origin.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AttrChar {
    /// Character value.
    pub value: char,
    /// Character origin.
    pub origin: Origin,
    /// Whether this character is quoted by another character.
    pub is_quoted: bool,
    /// Whether this is a quotation character that quotes another character.
    ///
    /// Note that a character can be both quoting and quoted. For example, the
    /// backslash in `"\$"` quotes the dollar and is quoted by the
    /// double-quotes.
    pub is_quoting: bool,
}

/// String of `AttrChar`s with the location of the originating word.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttrField {
    /// Value of the field.
    pub chars: Vec<AttrChar>,
    /// Location of the word this field resulted from.
    pub origin: Location,
}

impl AttrField {
    /// Convenience function performing [quote removal](super::quote_removal)
    /// and [attribute stripping](super::attr_strip) at once
    ///
    /// This function is a bit more efficient than calling
    /// [`remove_quotes`](super::quote_removal::remove_quotes) and
    /// [`strip`](super::attr_strip::Strip::strip) separately.
    pub fn remove_quotes_and_strip(self) -> Field {
        use super::attr_strip::Strip;
        use super::quote_removal::skip_quotes;
        let value = skip_quotes(self.chars).strip().collect();
        let origin = self.origin;
        Field { value, origin }
    }
}
