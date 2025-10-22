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

//! Attribute stripping for AttrChar

use super::attr::AttrChar;

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
