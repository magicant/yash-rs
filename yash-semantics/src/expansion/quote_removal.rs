// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Quote removal
//!
//! The quote removal is a step of the word expansion that removes quotes from
//! the field. Yash's notion of the quote removal is a bit different from that
//! of POSIX in that the [attribute stripping](super::attr_strip::Strip) is a
//! separate operation from the quote removal.
//!
//! There are two versions of quote removal implementation.
//! [`skip_quotes`] wraps an iterator of `AttrChar`s with another iterator that
//! removes quotes from iteration.
//! [`remove_quotes`] removes quotes from a mutable vector of `AttrChar`s.
//!
//! ```
//! # use yash_semantics::expansion::attr::{AttrChar, Origin};
//! # use yash_semantics::expansion::quote_removal::skip_quotes;
//! let a = AttrChar {
//!     value: '\\',
//!     origin: Origin::Literal,
//!     is_quoted: false,
//!     is_quoting: true,
//! };
//! let b = AttrChar {
//!     value: 'X',
//!     origin: Origin::Literal,
//!     is_quoted: true,
//!     is_quoting: false,
//! };
//! let input = [a, b];
//! let output = skip_quotes(input).collect::<Vec<_>>();
//! assert_eq!(output, [b]);
//! ```

use super::AttrChar;

/// Performs quote removal on an iterator.
///
/// This function returns an iterator that skips over quoting characters from
/// the original iterator.
pub fn skip_quotes<I>(iter: I) -> impl Iterator<Item = AttrChar>
where
    I: IntoIterator<Item = AttrChar>,
{
    iter.into_iter().filter(|c| !c.is_quoting)
}

/// Performs quote removal on a mutable vector of `AttrChar`s.
///
/// This function removes quoting characters from the vector.
pub fn remove_quotes(chars: &mut Vec<AttrChar>) {
    chars.retain(|c| !c.is_quoting)
}

#[cfg(test)]
mod tests {
    use super::super::Origin;
    use super::*;

    #[test]
    fn test_skip_quotes() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar {
            value: 'b',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let c = AttrChar {
            value: 'c',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: false,
        };
        let d = AttrChar {
            value: 'd',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: true,
        };
        let input = [a, b, c, d];
        let output = skip_quotes(input).collect::<Vec<_>>();
        assert_eq!(output, [a, c]);
    }

    #[test]
    fn test_remove_quotes() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar {
            value: 'b',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let c = AttrChar {
            value: 'c',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: false,
        };
        let d = AttrChar {
            value: 'd',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: true,
        };
        let mut chars = vec![a, b, c, d];
        remove_quotes(&mut chars);
        assert_eq!(chars, [a, c]);
    }
}
