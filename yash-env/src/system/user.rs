// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! Items for user management

use super::Result;
use crate::path::PathBuf;
use std::ffi::CStr;

#[cfg(unix)]
type RawUidDef = libc::uid_t;
#[cfg(not(unix))]
type RawUidDef = u32;

/// Raw user ID type
///
/// This is a type alias for the raw user ID type `uid_t` declared in the
/// [`libc`] crate. The exact representation of this type is platform-dependent
/// while POSIX requires the type to be an integer. On non-Unix platforms, this
/// type is hard-coded to `u32`.
///
/// User IDs are usually wrapped in the [`Uid`] type for better type safety, so
/// this type is not used directly in most cases.
pub type RawUid = RawUidDef;

/// User ID
///
/// This type implements the new type pattern for the raw user ID type
/// [`RawUid`]. The advantage of using this type is that it is more type-safe
/// than using the raw integer value directly.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Uid(pub RawUid);

#[cfg(unix)]
type RawGidDef = libc::gid_t;
#[cfg(not(unix))]
type RawGidDef = u32;

/// Raw group ID type
///
/// This is a type alias for the raw group ID type `gid_t` declared in the
/// [`libc`] crate. The exact representation of this type is platform-dependent
/// while POSIX requires the type to be an integer. On non-Unix platforms, this
/// type is hard-coded to `u32`.
///
/// Group IDs are usually wrapped in the [`Gid`] type for better type safety, so
/// this type is not used directly in most cases.
pub type RawGid = RawGidDef;

/// Group ID
///
/// This type implements the new type pattern for the raw group ID type
/// [`RawGid`]. The advantage of using this type is that it is more type-safe
/// than using the raw integer value directly.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Gid(pub RawGid);

/// Trait for getting user and group IDs
pub trait GetUid {
    /// Returns the real user ID of the current process.
    fn getuid(&self) -> Uid;

    /// Returns the effective user ID of the current process.
    fn geteuid(&self) -> Uid;

    /// Returns the real group ID of the current process.
    fn getgid(&self) -> Gid;

    /// Returns the effective group ID of the current process.
    fn getegid(&self) -> Gid;
}

/// Trait for getting user information
///
/// This trait declares methods for getting user information. `Pw` in the trait
/// name stands for "password", referring to the traditional Unix password
/// file that stores user account information.
pub trait GetPw {
    /// Returns the home directory path of the given user.
    ///
    /// Returns `Ok(None)` if the user is not found.
    fn getpwnam_dir(&self, name: &CStr) -> Result<Option<PathBuf>>;
}
