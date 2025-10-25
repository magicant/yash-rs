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

//! Word expansion components
//!
//! This module defines some types used in word expansion.
//! Some part of word expansion can be implemented independently of the whole
//! shell language semantics, and those parts are implemented in this module
//! for better modularity and reusability. Other parts that depend on the shell
//! language semantics are implemented in the
//! [`yash-semantics` crate](https://crates.io/crates/yash-semantics).

pub mod attr;
pub mod attr_strip;
pub mod quote_removal;
pub mod split;
