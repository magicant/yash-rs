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
use crate::path::{Path, PathBuf};
use crate::str::UnixStr;
use bitflags::bitflags;
use enumset::{EnumSet, EnumSetType};
use std::ffi::CStr;
use std::fmt::Debug;
use std::io::SeekFrom;

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

/// Metadata of a file
///
/// Implementations of this trait represent metadata of a file, such as its
/// type, permissions, owner, size, etc. The [`Fstat`] trait provides methods to
/// retrieve `Stat` objects for files.
pub trait Stat {
    /// Device ID
    #[must_use]
    fn dev(&self) -> u64;
    /// Inode number
    #[must_use]
    fn ino(&self) -> u64;
    /// File mode (permission bits)
    ///
    /// Note that this field does not include the file type bits.
    /// Use [`type`](Self::type) to get the file type.
    /// You can also use [`is_regular_file`](Self::is_regular_file) and other
    /// similar methods to check the file type.
    #[must_use]
    fn mode(&self) -> Mode;
    /// File type
    #[must_use]
    fn r#type(&self) -> FileType;
    /// Number of hard links
    #[must_use]
    fn nlink(&self) -> u64;
    /// User ID of the file owner
    #[must_use]
    fn uid(&self) -> Uid;
    /// Group ID of the file owner
    #[must_use]
    fn gid(&self) -> Gid;
    /// Size of the file in bytes
    #[must_use]
    fn size(&self) -> u64;
    // TODO: atime, mtime, ctime, (birthtime)

    /// Returns the device ID and inode number as a tuple.
    ///
    /// This method is useful for testing whether two `Stat` objects refer to
    /// the same file.
    #[inline(always)]
    #[must_use]
    fn identity(&self) -> (u64, u64) {
        (self.dev(), self.ino())
    }

    /// Whether the file is a regular file
    #[inline(always)]
    #[must_use]
    fn is_regular_file(&self) -> bool {
        self.r#type() == FileType::Regular
    }
    /// Whether the file is a directory
    #[inline(always)]
    #[must_use]
    fn is_directory(&self) -> bool {
        self.r#type() == FileType::Directory
    }
    /// Whether the file is a symbolic link
    #[inline(always)]
    #[must_use]
    fn is_symlink(&self) -> bool {
        self.r#type() == FileType::Symlink
    }
    /// Whether the file is a pipe
    #[inline(always)]
    #[must_use]
    fn is_fifo(&self) -> bool {
        self.r#type() == FileType::Fifo
    }
    /// Whether the file is a block device
    #[inline(always)]
    #[must_use]
    fn is_block_device(&self) -> bool {
        self.r#type() == FileType::BlockDevice
    }
    /// Whether the file is a character device
    #[inline(always)]
    #[must_use]
    fn is_character_device(&self) -> bool {
        self.r#type() == FileType::CharacterDevice
    }
    /// Whether the file is a socket
    #[inline(always)]
    #[must_use]
    fn is_socket(&self) -> bool {
        self.r#type() == FileType::Socket
    }
}

/// Trait for retrieving file metadata
///
/// See also [`IsExecutableFile`].
pub trait Fstat {
    /// Metadata type returned by [`fstat`](Self::fstat) and [`fstatat`](Self::fstatat)
    type Stat: Stat + Clone + Debug;

    /// Retrieves metadata of a file.
    ///
    /// This method wraps the [`fstat` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fstat.html).
    /// It takes a file descriptor and returns a `Stat` object containing the
    /// file metadata.
    fn fstat(&self, fd: Fd) -> Result<Self::Stat>;

    /// Retrieves metadata of a file.
    ///
    /// This method wraps the [`fstatat` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fstatat.html).
    /// It takes a directory file descriptor, a file path, and a flag indicating
    /// whether to follow symbolic links. It returns a `Stat` object containing
    /// the file metadata. The file path is interpreted relative to the
    /// directory represented by the directory file descriptor.
    fn fstatat(&self, dir_fd: Fd, path: &CStr, follow_symlinks: bool) -> Result<Self::Stat>;

    /// Whether there is a directory at the specified path.
    #[must_use]
    fn is_directory(&self, path: &CStr) -> bool {
        self.fstatat(AT_FDCWD, path, /* follow_symlinks */ true)
            .is_ok_and(|stat| stat.is_directory())
    }

    /// Tests if a file descriptor is a pipe.
    #[must_use]
    fn fd_is_pipe(&self, fd: Fd) -> bool {
        self.fstat(fd).is_ok_and(|stat| stat.is_fifo())
    }
}

/// Trait for checking if a file is executable
///
/// This trait declares the `is_executable_file` method, which checks whether a
/// filepath points to an executable regular file. This trait is separate from
/// the [`Fstat`] trait because the implementation depends on the `faccessat`
/// system call.
pub trait IsExecutableFile {
    /// Whether there is an executable regular file at the specified path.
    #[must_use]
    fn is_executable_file(&self, path: &CStr) -> bool;
}

/// File access mode of open file descriptions
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum OfdAccess {
    /// Open for reading only
    ReadOnly,
    /// Open for writing only
    WriteOnly,
    /// Open for reading and writing
    ReadWrite,
    /// Open for executing only (non-directory files)
    Exec,
    /// Open for searching only (directories)
    Search,
}

/// Options for opening file descriptors
///
/// A set of `OpenFlag` values can be passed to [`open`] to configure how the
/// file descriptor is opened. Some of the flags become the attributes of the
/// open file description created by the `open` function.
///
/// [`open`]: Open::open
#[derive(Debug, EnumSetType, Hash)]
#[non_exhaustive]
pub enum OpenFlag {
    /// Always write to the end of the file
    Append,
    /// Close the file descriptor upon execution of an exec family function
    CloseOnExec,
    /// Create the file if it does not exist
    Create,
    /// Fail if the file is not a directory
    Directory,
    /// Atomically create the file if it does not exist
    Exclusive,
    /// Do not make the opened terminal the controlling terminal for the process
    NoCtty,
    /// Do not follow symbolic links
    NoFollow,
    /// Open the file in non-blocking mode
    NonBlock,
    /// Wait until the written data is physically stored on the underlying
    /// storage device on each write
    Sync,
    /// Truncate the file to zero length
    Truncate,
}

/// Trait for opening files
pub trait Open {
    /// Opens a file descriptor.
    ///
    /// This is a thin wrapper around the [`open` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/open.html).
    ///
    /// This function returns a future because opening a pipeline or device file
    /// may block the calling task until another process opens the other end of
    /// the pipeline or the device file is ready.
    /// See the [module-level documentation](super) for details.
    fn open(
        &self,
        path: &CStr,
        access: OfdAccess,
        flags: EnumSet<OpenFlag>,
        mode: Mode,
    ) -> impl Future<Output = Result<Fd>> + use<Self>;

    /// Opens a file descriptor associated with an anonymous temporary file.
    ///
    /// This function works similarly to the `O_TMPFILE` flag specified to the
    /// `open` function.
    fn open_tmpfile(&self, parent_dir: &Path) -> Result<Fd>;

    /// Opens a directory for enumerating entries.
    ///
    /// This is a thin wrapper around the [`fdopendir` system
    /// function](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fdopendir.html).
    fn fdopendir(&self, fd: Fd) -> Result<impl Dir + use<Self>>;

    /// Opens a directory for enumerating entries.
    ///
    /// This is a thin wrapper around the [`opendir` system
    /// function](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fdopendir.html).
    fn opendir(&self, path: &CStr) -> Result<impl Dir + use<Self>>;
}

/// Trait for seeking within file descriptors
pub trait Seek {
    /// Moves the position of the open file description.
    ///
    /// This is a thin wrapper around the [`lseek` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/lseek.html).
    /// If successful, returns the new position from the beginning of the file.
    fn lseek(&self, fd: Fd, position: SeekFrom) -> Result<u64>;
}

/// Trait for getting and setting the file creation mask
pub trait Umask {
    /// Gets and sets the file creation mode mask.
    ///
    /// This is a thin wrapper around the [`umask` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/umask.html).
    /// It sets the mask to the given value and returns the previous mask.
    ///
    /// You cannot tell the current mask without setting a new one. If you only
    /// want to get the current mask, you need to set it back to the original
    /// value after getting it.
    fn umask(&self, new_mask: Mode) -> Mode;
}

/// Trait for getting the current working directory
///
/// See also [`Chdir`].
pub trait GetCwd {
    /// Returns the current working directory path.
    fn getcwd(&self) -> Result<PathBuf>;
}

/// Trait for changing the current working directory
///
/// See also [`GetCwd`].
pub trait Chdir {
    /// Changes the working directory.
    fn chdir(&self, path: &CStr) -> Result<()>;
}
