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
//! Pathname expansion (a.k.a. globbing) scans directories and produces pathnames matching the input field.
//!
//! # Pattern syntax
//!
//! An input field is split by `/`, and each component is parsed as a pattern
//! that may contain the following non-literal elements:
//!
//! - `?`
//! - `*`
//! - Character classes (a set of characters enclosed in brackets)
//!
//! Refer to the [`fnmatch`] crate for pattern syntax and semantics details.
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
