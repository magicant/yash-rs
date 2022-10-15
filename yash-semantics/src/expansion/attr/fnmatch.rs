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

//! Utility for parsing `AttrChar` strings as a fnmatch pattern

use super::AttrChar;
use yash_fnmatch::PatternChar;

/// Converts unquoted backslashes to quoting characters.
///
/// Sets the `is_quoting` flag of unquoted backslashes and the `is_quoted` flag
/// of their following characters.
pub fn apply_escapes(chars: &mut [AttrChar]) {
    for j in 1..chars.len() {
        let i = j - 1;
        if chars[i].value == '\\' && !chars[i].is_quoting && !chars[i].is_quoted {
            chars[i].is_quoting = true;
            chars[j].is_quoted = true;
        }
    }
}

/// Returns an iterator of `PatternChar`s from an `AttrChar` slice.
pub fn to_pattern_chars(chars: &[AttrChar]) -> impl Iterator<Item = PatternChar> + Clone + '_ {
    chars.iter().filter_map(|c| {
        if c.is_quoting {
            None
        } else if c.is_quoted {
            Some(PatternChar::Literal(c.value))
        } else {
            Some(PatternChar::Normal(c.value))
        }
    })
}
