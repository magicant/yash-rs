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
//! The field splitting divides a field into smaller parts delimited by a field
//! separator character. As a side effect, this operation filters out empty
//! fields that are not resulting from the field splitting itself.
//!
//! Fields are delimited by a field separator character, usually obtained from
//! the `$IFS` variable. Every occurrence of a non-whitespace separator delimits
//! a new field (which may be an empty field). One or more adjacent whitespace
//! separators in the middle of a field further split the field. Any separator
//! does not remain in the final results.
//!
//! Only [unquoted characters](AttrChar) having a `SoftExpansion`
//! [origin](Origin) are considered for delimiting. Other characters are not
//! subject to field splitting.
//!
//! TODO Code example
//!
//! TODO empty-last-field option

#[cfg(doc)]
use super::attr::AttrChar;
#[cfg(doc)]
use super::attr::Origin;
