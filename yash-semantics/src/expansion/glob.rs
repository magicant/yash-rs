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

//! Pathname expansion
//!
//! Pathname expansion (a.k.a. globbing) scans directories and produces
//! pathnames matching the input field.
//!
//! # Pattern syntax
//!
//! An input field is split by `/`, and each component is parsed as a pattern
//! that may contain the following non-literal elements:
//!
//! - `?`
//! - `*`
//! - Bracket expression (a set of characters enclosed in brackets)
//!
//! Refer to the [`yash-fnmatch`](yash_fnmatch) crate for pattern syntax and
//! semantics details.
//!
//! # Directory scanning
//!
//! The expansion scans directories corresponding to components containing any
//! non-literal elements above. The scan requires read permission for the
//! directory. For components that have only literal characters, no scan is
//! performed. Search permissions for all ancestor directories are needed to
//! check if the file exists referred to by the resulting pathname.
//!
//! # Results
//!
//! Pathname expansion returns pathnames that have matched the input pattern,
//! sorted alphabetically. Any errors are silently ignored. If directory
//! scanning produces no pathnames, the input pattern is returned intact. (TODO:
//! the null-glob option)
//!
//! If the input field contains no non-literal elements subject to pattern
//! matching at all, the result is the input intact.

use super::attr::AttrField;
use std::iter::Once;
use std::marker::PhantomData;
use yash_env::semantics::Field;
use yash_env::Env;

/// Iterator that provides results of parameter expansion
///
/// This iterator is created with the [`glob`] function.
pub struct Glob<'a> {
    /// Dummy to allow retaining a mutable reference to `Env` in the future
    ///
    /// The current [`glob`] implementation pre-computes all results before
    /// returning a `Glob`. The future implementation may optimize by using a
    /// [generator], which will need a real reference to `Env`.
    ///
    /// [generator]: https://github.com/rust-lang/rust/issues/43122
    env: PhantomData<&'a mut Env>,

    inner: Once<Field>,
}

impl Iterator for Glob<'_> {
    type Item = Field;
    fn next(&mut self) -> Option<Field> {
        self.inner.next()
    }
}

/// Performs parameter expansion.
///
/// This function returns an iterator that yields fields resulting from the
/// expansion.
pub fn glob(_env: &mut Env, field: AttrField) -> Glob {
    Glob {
        env: PhantomData,
        inner: std::iter::once(field.remove_quotes_and_strip()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expansion::{AttrChar, Origin};
    use yash_syntax::source::Location;

    fn dummy_attr_field(s: &str) -> AttrField {
        let chars = s
            .chars()
            .map(|c| AttrChar {
                value: c,
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            })
            .collect();
        let origin = Location::dummy("");
        AttrField { chars, origin }
    }

    #[test]
    fn literal_field() {
        let mut env = Env::new_virtual();
        let f = dummy_attr_field("abc");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "abc");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn quoting_characters_are_removed() {
        let mut env = Env::new_virtual();
        let mut f = dummy_attr_field("aXbcYde");
        f.chars[1].is_quoting = true;
        f.chars[4].is_quoting = true;
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "abcde");
        assert_eq!(i.next(), None);
    }

    // TODO AttrChar::is_quoted
    // TODO Origin::HardExpansion is literal
}
