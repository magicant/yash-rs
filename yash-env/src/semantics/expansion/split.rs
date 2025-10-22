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

//! Field splitting
//!
//! This module provides components for field splitting used in word expansion.
//! [`Ifs`] represents the field separator characters, and [`Class`] classifies
//! them into whitespace and non-whitespace characters. [`Ranges`] is an
//! iterator that yields the ranges of characters in a field separated by the
//! field separators. The [`split_into`] and [`split`] functions perform field
//! splitting on a given field using a given IFS.
//!
//! # Field splitting semantics
//!
//! Field splitting divides a field into smaller parts delimited by a field
//! separator character, which is usually obtained from the `$IFS` variable.
//! Every occurrence of a non-whitespace separator delimits a new field (which
//! may be an empty field). One or more adjacent whitespace separators in the
//! middle of a field further split the field. The separators are not included
//! in the final results.
//!
//! Only [unquoted characters](super::attr::AttrChar) having a
//! `SoftExpansion` [origin](super::attr::Origin) are considered for delimiting.
//! Other characters are not subject to field splitting.
//!
//! # Example
//!
//! ```
//! use yash_syntax::source::Location;
//! use yash_env::semantics::expansion::attr::{AttrChar, AttrField, Origin};
//! use yash_env::semantics::expansion::split::{Ifs, split};
//!
//! // We use this utility to prepare fields used in the examples below:
//! fn field(s: &str) -> AttrField {
//!     let chars = s.chars()
//!         .map(|c| AttrChar {
//!             value: c,
//!             origin: Origin::SoftExpansion,
//!             is_quoted: false,
//!             is_quoting: false,
//!         })
//!         .collect();
//!     let origin = Location::dummy("");
//!     AttrField { chars, origin }
//! }
//!
//! let ifs = Ifs::new(" -");
//!
//! // When there are no separators in the input, the result is the input itself:
//! let fields: Vec<AttrField> = split(field("abc"), &ifs);
//! assert_eq!(fields, [field("abc")]);
//!
//! // Whitespace separators are removed:
//! let fields: Vec<AttrField> = split(field("  abc   "), &ifs);
//! assert_eq!(fields, [field("abc")]);
//!
//! // An empty input yields no fields rather than an empty field:
//! let fields: Vec<AttrField> = split(field(""), &ifs);
//! assert_eq!(fields, []);
//!
//! // Whitespace separators split fields:
//! let fields: Vec<AttrField> = split(field("foo bar  baz"), &ifs);
//! assert_eq!(fields, [field("foo"), field("bar"), field("baz")]);
//!
//! // Non-whitespace separators each split fields, which may produce empty fields:
//! let fields: Vec<AttrField> = split(field("foo-bar--baz"), &ifs);
//! assert_eq!(fields, [field("foo"), field("bar"), field(""), field("baz")]);
//!
//! // Whitespace separators around non-whitespace separators are ignored:
//! let fields: Vec<AttrField> = split(field("foo - bar -  - baz"), &ifs);
//! assert_eq!(fields, [field("foo"), field("bar"), field(""), field("baz")]);
//!
//! // Trailing non-whitespace separators may seem special:
//! let fields: Vec<AttrField> = split(field("foo-bar"), &ifs);
//! assert_eq!(fields, [field("foo"), field("bar")]);
//! let fields: Vec<AttrField> = split(field("foo-bar-"), &ifs);
//! assert_eq!(fields, [field("foo"), field("bar")]);
//! let fields: Vec<AttrField> = split(field("foo-bar--"), &ifs);
//! assert_eq!(fields, [field("foo"), field("bar"), field("")]);
//! ```
//!
//! # The empty-last-field option
//!
//! TODO: Not yet supported

mod ifs;
mod ranges;

pub use self::ifs::{Class, Ifs};
pub use self::ranges::Ranges;

use super::attr::AttrField;

/// Performs field splitting and appends the result to a collection.
///
/// This function applies field splitting to the given field using the given IFS
/// and extends the given collection with the results. The resultant fields
/// share the same origin as the input field.
///
/// See also [`split`], which returns the results in a new collection rather
/// than extending an existing one.
pub fn split_into<R>(field: AttrField, ifs: &Ifs, results: &mut R)
where
    R: Extend<AttrField>,
{
    /*
    results.extend(
        ifs.ranges(field.chars.iter().copied())
            .map(|range| AttrField {
                chars: field.chars[range].to_vec(),
                origin: field.origin.clone(),
            }),
    );
    */

    // Optimize by reusing the original field for the last one.
    let mut ranges = ifs.ranges(field.chars.iter().copied()).peekable();
    while let Some(range) = ranges.next() {
        // TODO Use Extend::extend_one when stabilized (rust#72631)
        if ranges.peek().is_some() {
            results.extend(std::iter::once(AttrField {
                chars: field.chars[range].to_vec(),
                origin: field.origin.clone(),
            }));
        } else {
            let mut field = field;
            field.chars.truncate(range.end);
            field.chars.drain(..range.start);
            results.extend(std::iter::once(field));
            break;
        }
    }
}

/// Performs field splitting and returns the result in a new collection.
///
/// This function works similarly to [`split_into`], but returns the results in
/// a new collection.
pub fn split<R>(field: AttrField, ifs: &Ifs) -> R
where
    R: Default + Extend<AttrField>,
{
    let mut results = R::default();
    split_into(field, ifs, &mut results);
    results
}

#[cfg(test)]
mod tests {
    use super::super::attr::{AttrChar, Origin};
    use super::*;
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
    fn split_empty_field() {
        let field = dummy_attr_field("");
        let ifs = Ifs::default();
        let fields: Vec<AttrField> = split(field, &ifs);
        assert_eq!(fields, []);
    }

    #[test]
    fn split_no_change() {
        let field = dummy_attr_field("abc");
        let ifs = Ifs::default();
        let fields: Vec<AttrField> = split(field, &ifs);
        assert_eq!(fields, [dummy_attr_field("abc")]);
    }

    #[test]
    fn split_into_one_field() {
        let field = dummy_attr_field(" foo ");
        let ifs = Ifs::default();
        let fields: Vec<AttrField> = split(field, &ifs);
        assert_eq!(fields, [dummy_attr_field("foo")]);
    }

    #[test]
    fn split_into_two_fields() {
        let field = dummy_attr_field("foo  bar");
        let ifs = Ifs::default();
        let fields: Vec<AttrField> = split(field, &ifs);
        assert_eq!(fields, [dummy_attr_field("foo"), dummy_attr_field("bar")]);
    }

    #[test]
    fn split_into_many_fields() {
        let field = dummy_attr_field(" one two  three four  ");
        let ifs = Ifs::default();
        let fields: Vec<AttrField> = split(field, &ifs);
        assert_eq!(
            fields,
            [
                dummy_attr_field("one"),
                dummy_attr_field("two"),
                dummy_attr_field("three"),
                dummy_attr_field("four")
            ]
        );
    }
}
