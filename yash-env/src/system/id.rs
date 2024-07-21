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

//! Definitions of ID types

#[cfg(unix)]
type RawUidDef = nix::libc::uid_t;
#[cfg(not(unix))]
type RawUidDef = u32;

/// Raw user ID type
///
/// This is a type alias for the raw user ID type `uid_t` declared in the
/// [`libc`] crate. The exact representation of this type is platform-dependent
/// while POSIX requires the type to be an integer. On non-Unix platforms, this
/// type is hard-coded to `u32`.
///
/// [`libc`]: nix::libc
pub type RawUid = RawUidDef;

/// User ID
///
/// This type implements the new type pattern for the raw user ID type
/// [`RawUid`]. The advantage of using this type is that it is more type-safe
/// than using the raw integer value directly.
///
/// [`libc`]: nix::libc
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Uid(pub RawUid);

#[cfg(unix)]
type RawGidDef = nix::libc::gid_t;
#[cfg(not(unix))]
type RawGidDef = u32;

/// Raw group ID type
///
/// This is a type alias for the raw group ID type `gid_t` declared in the
/// [`libc`] crate. The exact representation of this type is platform-dependent
/// while POSIX requires the type to be an integer. On non-Unix platforms, this
/// type is hard-coded to `u32`.
///
/// [`libc`]: nix::libc
pub type RawGid = RawGidDef;

/// Group ID
///
/// This type implements the new type pattern for the raw group ID type
/// [`RawGid`]. The advantage of using this type is that it is more type-safe
/// than using the raw integer value directly.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Gid(pub RawGid);
