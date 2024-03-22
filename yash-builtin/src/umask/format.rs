// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Formatting the file mode creating mask for printing

/// Formats the file mode creation mask in symbolic notation.
#[must_use]
pub fn format_symbolic(mask: u16) -> String {
    let mut result = String::with_capacity(18);
    result.push_str("u=");
    if mask & 0o400 != 0 {
        result.push('r');
    }
    if mask & 0o200 != 0 {
        result.push('w');
    }
    if mask & 0o100 != 0 {
        result.push('x');
    }
    result.push_str(",g=");
    if mask & 0o40 != 0 {
        result.push('r');
    }
    if mask & 0o20 != 0 {
        result.push('w');
    }
    if mask & 0o10 != 0 {
        result.push('x');
    }
    result.push_str(",o=");
    if mask & 0o4 != 0 {
        result.push('r');
    }
    if mask & 0o2 != 0 {
        result.push('w');
    }
    if mask & 0o1 != 0 {
        result.push('x');
    }
    result
}

#[test]
fn empty() {
    assert_eq!(format_symbolic(0), "u=,g=,o=");
}

#[test]
fn full() {
    assert_eq!(format_symbolic(0o777), "u=rwx,g=rwx,o=rwx");
}

#[test]
fn combination() {
    assert_eq!(format_symbolic(0o124), "u=x,g=w,o=r");
    assert_eq!(format_symbolic(0o241), "u=w,g=r,o=x");
    assert_eq!(format_symbolic(0o412), "u=r,g=x,o=w");
}
