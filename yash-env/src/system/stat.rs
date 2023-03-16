// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Items about file status

/// Result of "stat" family functions
///
/// The fields of this struct have platform-specific types, which may differ
/// depending on the environment.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct FileStat {
    /// Device ID
    pub dev: libc::dev_t,
    /// I-node ID
    pub ino: libc::ino_t,
    /// File access permissions and file type
    pub mode: libc::mode_t,
    /// File length in bytes
    pub size: libc::off_t,
}

impl From<libc::stat> for FileStat {
    fn from(stat: libc::stat) -> Self {
        Self {
            dev: stat.st_dev,
            ino: stat.st_ino,
            mode: stat.st_mode,
            size: stat.st_size,
        }
    }
}

impl FileStat {
    /// Tests whether this file is a regular file.
    #[must_use]
    pub const fn is_regular(&self) -> bool {
        self.mode & libc::S_IFMT == libc::S_IFREG
    }

    /// Tests whether the two files are the same file.
    #[must_use]
    pub const fn is_same_file_as(&self, other: &Self) -> bool {
        self.dev == other.dev && self.ino == other.ino
    }
}
