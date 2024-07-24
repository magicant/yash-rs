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

//! Items about file systems

use super::Result;
use std::ffi::OsStr;
use std::fmt::Debug;
use yash_syntax::syntax::Fd;

#[cfg(unix)]
const RAW_AT_FDCWD: i32 = nix::libc::AT_FDCWD;
#[cfg(not(unix))]
const RAW_AT_FDCWD: i32 = -100;

/// Sentinel for the current working directory
///
/// This value can be passed to system calls named "*at" such as
/// [`fstatat`](super::System::fstatat).
pub const AT_FDCWD: Fd = Fd(RAW_AT_FDCWD);

/// Metadata of a file contained in a directory
///
/// `DirEntry` objects are enumerated by a [`Dir`] implementor.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct DirEntry<'a> {
    /// Filename
    pub name: &'a OsStr,
}

/// Trait for enumerating directory entries
///
/// An implementor of `Dir` may retain a file descriptor (or any other resource
/// alike) to access the underlying system and obtain entry information. The
/// file descriptor is released when the implementor object is dropped.
pub trait Dir: Debug {
    /// Returns the next directory entry.
    fn next(&mut self) -> Result<Option<DirEntry>>;
}

#[cfg(unix)]
type RawModeDef = nix::libc::mode_t;
#[cfg(not(unix))]
type RawModeDef = u32;

/// Raw file permission bits type
///
/// This is a type alias for the raw file permission bits type `mode_t` declared
/// in the [`libc`] crate. The exact representation of this type is
/// platform-dependent while POSIX requires the type to be an integer. On
/// non-Unix platforms, this type is hard-coded to `u32`.
///
/// File permission bits are usually wrapped in the [`Mode`] type for better type
/// safety, so this type is not used directly in most cases.
///
/// [`libc`]: nix::libc
pub type RawMode = RawModeDef;

/// File permission bits
///
/// This type implements the new type pattern for the raw file permission bits
/// type [`RawMode`]. The advantage of using this type is that it is more
/// type-safe than using the raw integer value directly.
#[derive(Copy, Clone, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct Mode(pub RawMode);

impl Debug for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mode({:#o})", self.0)
    }
}

/// The default mode is `0o644`, not `0o000`.
impl Default for Mode {
    fn default() -> Mode {
        Mode(0o644)
    }
}
