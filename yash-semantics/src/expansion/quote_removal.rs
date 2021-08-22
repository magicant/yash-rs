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

//! Quote removal.

use super::AttrChar;
use super::AttrField;
use super::Field;

/// Quote removal.
///
/// The quote removal is a step of the word expansion that removes quotes from
/// the field. The [`do_quote_removal`](Self::do_quote_removal) function
/// converts an [`AttrChar`] string to a regular string.
pub trait QuoteRemoval {
    /// Return type of [`do_quote_removal`](Self::do_quote_removal).
    type Output;

    /// Performs the quote removal on `self`.
    ///
    /// TODO Add a parameter to specify how characters in the result should be
    /// escaped.
    fn do_quote_removal(self) -> Self::Output;
}

impl QuoteRemoval for &[AttrChar] {
    type Output = String;
    fn do_quote_removal(self) -> String {
        // TODO Remove quotes correctly
        self.iter()
            .filter(|c| !c.is_quoting)
            .map(|c| c.value)
            .collect()
    }
}

impl QuoteRemoval for AttrField {
    type Output = Field;
    fn do_quote_removal(self) -> Field {
        let AttrField { chars, origin } = self;
        let value = chars.do_quote_removal();
        Field { value, origin }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Origin;
    use super::*;

    #[test]
    fn quote_removal() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar {
            value: 'b',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: false,
        };
        let c = AttrChar {
            value: 'c',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let d = AttrChar {
            value: 'd',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: true,
        };
        let input = [a, b, c, d];
        let result = input.do_quote_removal();
        assert_eq!(result, "ab");
    }
}
