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

//! Extension to [`crate::system::resource`] for the real system

use super::super::resource::Resource;

impl Resource {
    /// Returns the platform-specific constant value of this resource type.
    ///
    /// This method returns `None` if the resource type is not available on the
    /// current platform.
    #[must_use]
    pub(super) const fn as_raw_type(&self) -> Option<std::ffi::c_int> {
        match *self {
            #[cfg(not(any(target_env = "newlib", target_os = "redox")))]
            Self::AS => Some(libc::RLIMIT_AS as _),
            Self::CORE => Some(libc::RLIMIT_CORE as _),
            Self::CPU => Some(libc::RLIMIT_CPU as _),
            Self::DATA => Some(libc::RLIMIT_DATA as _),
            Self::FSIZE => Some(libc::RLIMIT_FSIZE as _),
            #[cfg(target_os = "freebsd")]
            Self::KQUEUES => Some(libc::RLIMIT_KQUEUES as _),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "emscripten"))]
            Self::LOCKS => Some(libc::RLIMIT_LOCKS as _),
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "watchos",
                target_os = "freebsd",
                target_os = "dragonfly",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "linux",
                target_os = "android",
                target_os = "emscripten",
                target_os = "nto"
            ))]
            Self::MEMLOCK => Some(libc::RLIMIT_MEMLOCK as _),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "emscripten"))]
            Self::MSGQUEUE => Some(libc::RLIMIT_MSGQUEUE as _),
            #[cfg(any(target_os = "linux", target_os = "android"))]
            Self::NICE => Some(libc::RLIMIT_NICE as _),
            Self::NOFILE => Some(libc::RLIMIT_NOFILE as _),
            #[cfg(any(
                target_os = "aix",
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "watchos",
                target_os = "freebsd",
                target_os = "dragonfly",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "linux",
                target_os = "android",
                target_os = "emscripten",
                target_os = "nto"
            ))]
            Self::NPROC => Some(libc::RLIMIT_NPROC as _),
            #[cfg(any(
                target_os = "aix",
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "watchos",
                target_os = "freebsd",
                target_os = "dragonfly",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "linux",
                target_os = "android",
                target_os = "emscripten",
                target_os = "nto"
            ))]
            Self::RSS => Some(libc::RLIMIT_RSS as _),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "emscripten"))]
            Self::RTPRIO => Some(libc::RLIMIT_RTPRIO as _),
            #[cfg(target_os = "linux")]
            Self::RTTIME => Some(libc::RLIMIT_RTTIME as _),
            #[cfg(any(target_os = "freebsd", target_os = "dragonfly", target_os = "netbsd"))]
            Self::SBSIZE => Some(libc::RLIMIT_SBSIZE as _),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "emscripten"))]
            Self::SIGPENDING => Some(libc::RLIMIT_SIGPENDING as _),
            Self::STACK => Some(libc::RLIMIT_STACK as _),
            #[cfg(target_os = "freebsd")]
            Self::SWAP => Some(libc::RLIMIT_SWAP as _),
            _ => None,
        }
    }
}
