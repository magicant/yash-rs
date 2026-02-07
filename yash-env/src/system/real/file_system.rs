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

//! Extension to [`crate::system::file_system`] for the real system

use super::super::{FileType, Gid, Mode, RawMode, Uid};
use std::mem::MaybeUninit;

impl FileType {
    #[must_use]
    pub(super) const fn from_raw(mode: RawMode) -> Self {
        match mode & libc::S_IFMT {
            libc::S_IFREG => Self::Regular,
            libc::S_IFDIR => Self::Directory,
            libc::S_IFLNK => Self::Symlink,
            libc::S_IFIFO => Self::Fifo,
            libc::S_IFBLK => Self::BlockDevice,
            libc::S_IFCHR => Self::CharacterDevice,
            libc::S_IFSOCK => Self::Socket,
            _ => Self::Other,
        }
    }
}

/// Metadata of a file in the real file system
///
/// This is an implementation of the [`Stat` trait](super::super::Stat) for the
/// [`RealSystem`](super::RealSystem).
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct Stat(MaybeUninit<libc::stat>);
// TODO: The auto-derived Debug implementation does not provide useful information.
// Consider implementing a custom Debug that shows the contents.

impl Stat {
    /// Converts a raw `stat` structure to a `Stat` object.
    ///
    /// This function assumes the `stat` structure to be initialized by the
    /// `stat` system call, but it is passed as `MaybeUninit` because of
    /// possible padding or extension fields in the structure which may not be
    /// initialized by the system call.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the provided `stat` structure is properly
    /// initialized by a system call like `stat`, `fstat`, or `lstat`.
    #[must_use]
    pub(super) const unsafe fn from_raw(stat: MaybeUninit<libc::stat>) -> Self {
        Self(stat)
    }
}

// The actual types of the fields in `libc::stat` may vary across platforms,
// so some casts may be necessary.
#[allow(clippy::unnecessary_cast)]
impl super::super::Stat for Stat {
    #[inline(always)]
    fn dev(&self) -> u64 {
        (unsafe { (*self.0.as_ptr()).st_dev }) as u64
    }
    #[inline(always)]
    fn ino(&self) -> u64 {
        (unsafe { (*self.0.as_ptr()).st_ino }) as u64
    }
    #[inline(always)]
    fn mode(&self) -> Mode {
        let raw_mode = unsafe { (*self.0.as_ptr()).st_mode };
        Mode::from_bits_truncate(raw_mode)
    }
    #[inline(always)]
    fn r#type(&self) -> FileType {
        let raw_mode = unsafe { (*self.0.as_ptr()).st_mode };
        FileType::from_raw(raw_mode)
    }
    #[inline(always)]
    fn nlink(&self) -> u64 {
        (unsafe { (*self.0.as_ptr()).st_nlink }) as u64
    }
    #[inline(always)]
    fn uid(&self) -> Uid {
        Uid(unsafe { (*self.0.as_ptr()).st_uid })
    }
    #[inline(always)]
    fn gid(&self) -> Gid {
        Gid(unsafe { (*self.0.as_ptr()).st_gid })
    }
    #[inline(always)]
    fn size(&self) -> u64 {
        (unsafe { (*self.0.as_ptr()).st_size }) as u64
    }

    fn is_regular_file(&self) -> bool {
        let raw_mode = unsafe { (*self.0.as_ptr()).st_mode };
        raw_mode & libc::S_IFMT == libc::S_IFREG
    }
    fn is_directory(&self) -> bool {
        let raw_mode = unsafe { (*self.0.as_ptr()).st_mode };
        raw_mode & libc::S_IFMT == libc::S_IFDIR
    }
    fn is_symlink(&self) -> bool {
        let raw_mode = unsafe { (*self.0.as_ptr()).st_mode };
        raw_mode & libc::S_IFMT == libc::S_IFLNK
    }
    fn is_fifo(&self) -> bool {
        let raw_mode = unsafe { (*self.0.as_ptr()).st_mode };
        raw_mode & libc::S_IFMT == libc::S_IFIFO
    }
    fn is_block_device(&self) -> bool {
        let raw_mode = unsafe { (*self.0.as_ptr()).st_mode };
        raw_mode & libc::S_IFMT == libc::S_IFBLK
    }
    fn is_character_device(&self) -> bool {
        let raw_mode = unsafe { (*self.0.as_ptr()).st_mode };
        raw_mode & libc::S_IFMT == libc::S_IFCHR
    }
    fn is_socket(&self) -> bool {
        let raw_mode = unsafe { (*self.0.as_ptr()).st_mode };
        raw_mode & libc::S_IFMT == libc::S_IFSOCK
    }
}
