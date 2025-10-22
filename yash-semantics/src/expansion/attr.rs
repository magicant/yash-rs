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

pub(crate) mod fnmatch;

use yash_env::semantics::Field;
use yash_syntax::source::Location;

// Re-export items from yash_env for backward compatibility
pub use yash_env::expansion::attr::{AttrChar, Origin};

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
