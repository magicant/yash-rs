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

use super::super::{FileType, Gid, Mode, RawMode, Stat, Uid};
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

impl Stat {
    /// Converts a raw `stat` structure to a `Stat` object.
    ///
    /// This function requires the `stat` structure to be initialized, but it is
    /// passed as `MaybeUninit` because of possible padding or extension fields
    /// in the structure which may not be initialized by the `stat` system call.
    #[must_use]
    pub(super) const fn from_raw(stat: &MaybeUninit<libc::stat>) -> Self {
        let ptr = stat.as_ptr();
        let raw_mode = unsafe { (&raw const (*ptr).st_mode).read() };
        Self {
            dev: unsafe { (&raw const (*ptr).st_dev).read() } as _,
            ino: unsafe { (&raw const (*ptr).st_ino).read() } as _,
            mode: Mode::from_bits_truncate(raw_mode),
            r#type: FileType::from_raw(raw_mode),
            nlink: unsafe { (&raw const (*ptr).st_nlink).read() } as _,
            uid: Uid(unsafe { (&raw const (*ptr).st_uid).read() }),
            gid: Gid(unsafe { (&raw const (*ptr).st_gid).read() }),
            size: unsafe { (&raw const (*ptr).st_size).read() } as _,
        }
    }
}
