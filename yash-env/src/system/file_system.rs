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

use super::{Gid, Result, Uid};
use crate::io::Fd;
use crate::str::UnixStr;
use bitflags::bitflags;
use std::ffi::CStr;
use std::fmt::Debug;

#[cfg(unix)]
const RAW_AT_FDCWD: i32 = libc::AT_FDCWD;
#[cfg(not(unix))]
const RAW_AT_FDCWD: i32 = -100;

/// Sentinel for the current working directory
///
/// This value can be passed to system calls named "*at" such as
/// [`fstatat`](super::Fstat::fstatat).
pub const AT_FDCWD: Fd = Fd(RAW_AT_FDCWD);

/// Metadata of a file contained in a directory
///
/// `DirEntry` objects are enumerated by a [`Dir`] implementor.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct DirEntry<'a> {
    /// Filename
    pub name: &'a UnixStr,
}

/// Trait for enumerating directory entries
///
/// An implementor of `Dir` may retain a file descriptor (or any other resource
/// alike) to access the underlying system and obtain entry information. The
/// file descriptor is released when the implementor object is dropped.
pub trait Dir: Debug {
    /// Returns the next directory entry.
    fn next(&mut self) -> Result<Option<DirEntry<'_>>>;
}

#[cfg(unix)]
type RawModeDef = libc::mode_t;
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
pub type RawMode = RawModeDef;

/// File permission bits
///
/// This type implements the new type pattern for the raw file permission bits
/// type [`RawMode`]. The advantage of using this type is that it is more
/// type-safe than using the raw integer value directly.
///
/// This type only defines the permission bits and does not include the file
/// type bits (e.g., regular file, directory, symbolic link, etc.). The file
/// types are represented by the [`FileType`] enum.
#[derive(Copy, Clone, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct Mode(RawMode);

bitflags! {
    impl Mode: RawMode {
        /// User read permission (`0o400`)
        const USER_READ = 0o400;
        /// User write permission (`0o200`)
        const USER_WRITE = 0o200;
        /// User execute permission (`0o100`)
        const USER_EXEC = 0o100;
        /// User read, write, and execute permissions (`0o700`)
        const USER_ALL = 0o700;
        /// Group read permission (`0o040`)
        const GROUP_READ = 0o040;
        /// Group write permission (`0o020`)
        const GROUP_WRITE = 0o020;
        /// Group execute permission (`0o010`)
        const GROUP_EXEC = 0o010;
        /// Group read, write, and execute permissions (`0o070`)
        const GROUP_ALL = 0o070;
        /// Other read permission (`0o004`)
        const OTHER_READ = 0o004;
        /// Other write permission (`0o002`)
        const OTHER_WRITE = 0o002;
        /// Other execute permission (`0o001`)
        const OTHER_EXEC = 0o001;
        /// Other read, write, and execute permissions (`0o007`)
        const OTHER_ALL = 0o007;
        /// All read permission (`0o444`)
        const ALL_READ = 0o444;
        /// All write permission (`0o222`)
        const ALL_WRITE = 0o222;
        /// All execute permission (`0o111`)
        const ALL_EXEC = 0o111;
        /// All combinations of (user, group, other) × (read, write, execute)
        ///
        /// Note that this is equivalent to `Mode::USER_ALL | Mode::GROUP_ALL |
        /// Mode::OTHER_ALL` and does not include the sticky bit, the
        /// set-user-ID bit, or the set-group-ID bit.
        const ALL_9 = 0o777;
        /// Set-user-ID bit (`0o4000`)
        const SET_USER_ID = 0o4000;
        /// Set-group-ID bit (`0o2000`)
        const SET_GROUP_ID = 0o2000;
        /// Sticky bit (`0o1000`)
        const STICKY = 0o1000;
    }
}

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

/// File type
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum FileType {
    /// Regular file
    Regular,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
    /// Pipe
    Fifo,
    /// Block special device file
    BlockDevice,
    /// Character special device file
    CharacterDevice,
    /// Socket
    Socket,
    /// Other file type, including unknown file types
    Other,
}

/// File status
///
/// This type is a collection of file status information. It is similar to the
/// `stat` structure defined in the POSIX standard, but it is simplified and
/// does not include all fields of the `stat` structure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct Stat {
    /// Device ID
    pub dev: u64,
    /// Inode number
    pub ino: u64,
    /// Access permissions
    ///
    /// Note that this field does not include the file type bits.
    /// The file type is stored in the `type` field.
    pub mode: Mode,
    /// File type
    pub r#type: FileType,
    /// Number of hard links
    pub nlink: u64,
    /// User ID of the file owner
    pub uid: Uid,
    /// Group ID of the file owner
    pub gid: Gid,
    /// Length of the file in bytes
    pub size: u64,
    // TODO: atime, mtime, ctime, (birthtime)
}

impl Stat {
    /// Returns the device ID and inode number as a tuple
    ///
    /// This method is useful for testing whether two `Stat` objects refer to
    /// the same file.
    #[inline]
    #[must_use]
    pub const fn identity(&self) -> (u64, u64) {
        (self.dev, self.ino)
    }
}

/// Trait for retrieving file metadata
pub trait Fstat {
    /// Retrieves metadata of a file.
    ///
    /// This method wraps the [`fstat` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fstat.html).
    /// It takes a file descriptor and returns a `Stat` object containing the
    /// file metadata.
    fn fstat(&self, fd: Fd) -> Result<Stat>;

    /// Retrieves metadata of a file.
    ///
    /// This method wraps the [`fstatat` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fstatat.html).
    /// It takes a directory file descriptor, a file path, and a flag indicating
    /// whether to follow symbolic links. It returns a `Stat` object containing
    /// the file metadata. The file path is interpreted relative to the
    /// directory represented by the directory file descriptor.
    fn fstatat(&self, dir_fd: Fd, path: &CStr, follow_symlinks: bool) -> Result<Stat>;

    /// Whether there is a directory at the specified path.
    #[must_use]
    fn is_directory(&self, path: &CStr) -> bool {
        self.fstatat(AT_FDCWD, path, /* follow_symlinks */ true)
            .is_ok_and(|stat| stat.r#type == FileType::Directory)
    }
}
