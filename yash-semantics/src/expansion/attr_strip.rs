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

//! Attribute stripping
//!
//! The attribute stripping is the final step of the word expansion that
//! converts [`AttrChar`] to plain `char`. The conversion is performed by
//! implementors of the [`Strip`] trait.
//!
//! ```
//! # use yash_semantics::expansion::attr::{AttrChar, Origin};
//! # use yash_semantics::expansion::attr_strip::Strip;
//! let c = AttrChar {
//!     value: 'X',
//!     origin: Origin::Literal,
//!     is_quoted: false,
//!     is_quoting: false,
//! };
//! assert_eq!(c.strip(), 'X');
//! ```

use super::attr::AttrChar;
use super::attr::AttrField;
use yash_env::semantics::Field;

/// Trait for performing attribute stripping.
pub trait Strip {
    /// Result of attribute stripping
    type Output;

    /// Performs attribute stripping.
    ///
    /// Converts an attributed character into a plain character.
    ///
    /// Note that this function does not perform quote removal.
    #[must_use]
    fn strip(self) -> Self::Output;
}

/// Performs attribute stripping on an attributed character.
impl Strip for AttrChar {
    type Output = char;
    fn strip(self) -> char {
        self.value
    }
}

/// Iterator wrapper that performs attribute stripping on items
///
/// An `Iter` is created by calling the [`Strip::strip`] method on an
/// iterator that yields items that also implement `Strip`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Iter<I>(I);

impl<I> Iterator for Iter<I>
where
    I: Iterator,
    <I as Iterator>::Item: Strip,
{
    type Item = <<I as Iterator>::Item as Strip>::Output;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(Strip::strip)
    }
}

/// Performs attribute stripping on items of an iterator.
impl<I> Strip for I
where
    I: Iterator,
    <I as Iterator>::Item: Strip,
{
    type Output = Iter<I>;
    fn strip(self) -> Iter<I> {
        Iter(self)
    }
}

/// Performs attribute stripping on the field value.
impl Strip for AttrField {
    type Output = Field;
    fn strip(self) -> Field {
        let value = self.chars.into_iter().strip().collect();
        let origin = self.origin;
        Field { value, origin }
    }
}

/// Performs attribute stripping on the field value.
impl Strip for &AttrField {
    type Output = Field;
    fn strip(self) -> Field {
        let value = self.chars.iter().copied().strip().collect();
        let origin = self.origin.clone();
        Field { value, origin }
    }
}

#[cfg(test)]
mod tests {
    use super::super::attr::Origin;
    use super::*;
    use yash_syntax::source::Location;

    #[test]
    fn attr_field_strip() {
        let origin = Location::dummy("foo");
        let field = AttrField {
            chars: vec![
                AttrChar {
                    value: 'a',
                    origin: Origin::Literal,
                    is_quoted: true,
                    is_quoting: false,
                },
                AttrChar {
                    value: 'x',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: true,
                },
            ],
            origin: origin.clone(),
        };

        let stripped = (&field).strip();
        let expected = Field {
            value: "ax".to_string(),
            origin,
        };
        assert_eq!(stripped, expected);

        let stripped = field.strip();
        assert_eq!(stripped, expected);
    }
}
