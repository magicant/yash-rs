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

//! Extension to [`crate::system::open_flag`] for the real system

use crate::system::open_flag::*;
use std::ffi::c_int;

impl OfdAccess {
    #[must_use]
    pub(super) fn to_real_flag(self) -> Option<c_int> {
        match self {
            Self::ReadOnly => Some(libc::O_RDONLY),
            Self::WriteOnly => Some(libc::O_WRONLY),
            Self::ReadWrite => Some(libc::O_RDWR),
            // TODO Support O_EXEC, O_PATH and O_SEARCH
            Self::Exec | Self::Search => None,
        }
    }

    #[must_use]
    pub(super) fn from_real_flag(flags: c_int) -> Self {
        match flags & libc::O_ACCMODE {
            libc::O_RDONLY => Self::ReadOnly,
            libc::O_WRONLY => Self::WriteOnly,
            libc::O_RDWR => Self::ReadWrite,
            _ => Self::Exec, // TODO Support O_PATH and O_SEARCH
        }
    }
}

impl OpenFlag {
    #[must_use]
    pub(super) fn to_real_flag(self) -> Option<c_int> {
        match self {
            Self::Append => Some(libc::O_APPEND),
            Self::CloseOnExec => Some(libc::O_CLOEXEC),
            Self::Create => Some(libc::O_CREAT),
            Self::Directory => Some(libc::O_DIRECTORY),
            Self::Exclusive => Some(libc::O_EXCL),
            #[cfg(not(any(target_env = "newlib", target_os = "redox")))]
            Self::NoCtty => Some(libc::O_NOCTTY),
            #[cfg(any(target_env = "newlib", target_os = "redox"))]
            Self::NoCtty => None,
            Self::NoFollow => Some(libc::O_NOFOLLOW),
            Self::NonBlock => Some(libc::O_NONBLOCK),
            #[cfg(not(target_os = "redox"))]
            Self::Sync => Some(libc::O_SYNC),
            #[cfg(target_os = "redox")]
            Self::Sync => None,
            Self::Truncate => Some(libc::O_TRUNC),
        }
    }
}
