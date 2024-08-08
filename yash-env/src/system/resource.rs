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

//! Resource types and limits
//!
//! This module defines resource types and their limit values that are used in
//! [`getrlimit`] and [`setrlimit`].
//!
//! [`getrlimit`]: super::System::getrlimit
//! [`setrlimit`]: super::System::setrlimit

#[cfg(unix)]
type RawLimit = nix::libc::rlim_t;
#[cfg(not(unix))]
type RawLimit = u64;

/// Unsigned integer type for resource limits
///
/// The size of this type may vary depending on the platform.
pub type Limit = RawLimit;

#[cfg(unix)]
const RLIM_INFINITY: Limit = nix::libc::RLIM_INFINITY;
#[cfg(not(unix))]
const RLIM_INFINITY: Limit = Limit::MAX;

/// Constant to specify an unlimited resource limit
///
/// The value of this constant is platform-specific.
pub const INFINITY: Limit = RLIM_INFINITY;

// No platforms are known to define `RLIM_SAVED_CUR` and `RLIM_SAVED_MAX` that
// have different values from `RLIM_INFINITY`, so they are not defined here.

// When adding a new resource type, also update the yash_builtin::ulimit::resource module.

/// Resource type definition
///
/// A `Resource` value represents a resource whose limit can be retrieved or
/// set using `getrlimit` and `setrlimit`.
///
/// This enum contains all possible resource types that may or may not be
/// available depending on the platform. To see if a resource is available on
/// the current platform, check the result of
/// [`as_raw_type`](Self::as_raw_type).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Resource {
    /// Maximum total memory size of the process
    AS,
    /// Maximum size of a core file created by a terminated process
    CORE,
    /// Maximum amount of CPU time the process can consume
    CPU,
    /// Maximum size of a data segment of the process
    DATA,
    /// Maximum size of a file the process can create
    FSIZE,
    /// Maximum number of kernel event queues (kqueues)
    KQUEUES,
    /// Maximum number of file locks the process can hold
    LOCKS,
    /// Maximum size of memory locked into RAM
    MEMLOCK,
    /// Maximum total size of POSIX message queues
    MSGQUEUE,
    /// Maximum process priority
    ///
    /// This resource specifies the highest priority that a process can set using
    /// `setpriority` or `nice`. When the resource value is set to *n*, the process
    /// can lower its nice value (that is, raise the priority) to (20 - *n*).
    NICE,
    /// Maximum number of open files in the process
    NOFILE,
    /// Maximum number of processes the user can run
    NPROC,
    /// Maximum physical memory size of the process
    RSS,
    /// Maximum real-time priority
    RTPRIO,
    /// Maximum amount of CPU time the process can consume in real-time
    /// scheduling mode without a blocking system call (microseconds)
    RTTIME,
    /// Maximum size of the socket buffer
    SBSIZE,
    /// Maximum number of signals that can be queued to the process
    SIGPENDING,
    /// Maximum size of the process stack
    STACK,
    /// Maximum size of the swap space that can be used by the user
    SWAP,
}

impl Resource {
    /// Slice of all resource types (including those not available on the current platform)
    pub const ALL: &'static [Resource] = &[
        Self::AS,
        Self::CORE,
        Self::CPU,
        Self::DATA,
        Self::FSIZE,
        Self::KQUEUES,
        Self::LOCKS,
        Self::MEMLOCK,
        Self::MSGQUEUE,
        Self::NICE,
        Self::NOFILE,
        Self::NPROC,
        Self::RSS,
        Self::RTPRIO,
        Self::RTTIME,
        Self::SBSIZE,
        Self::SIGPENDING,
        Self::STACK,
        Self::SWAP,
    ];

    /// Returns the platform-specific constant value of this resource type.
    ///
    /// This method returns `None` if the resource type is not available on the
    /// current platform.
    #[must_use]
    pub const fn as_raw_type(&self) -> Option<std::ffi::c_int> {
        match *self {
            #[cfg(not(any(target_env = "newlib", target_os = "redox")))]
            Self::AS => Some(nix::libc::RLIMIT_AS as _),
            Self::CORE => Some(nix::libc::RLIMIT_CORE as _),
            Self::CPU => Some(nix::libc::RLIMIT_CPU as _),
            Self::DATA => Some(nix::libc::RLIMIT_DATA as _),
            Self::FSIZE => Some(nix::libc::RLIMIT_FSIZE as _),
            #[cfg(target_os = "freebsd")]
            Self::KQUEUES => Some(nix::libc::RLIMIT_KQUEUES as _),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "emscripten"))]
            Self::LOCKS => Some(nix::libc::RLIMIT_LOCKS as _),
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
            Self::MEMLOCK => Some(nix::libc::RLIMIT_MEMLOCK as _),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "emscripten"))]
            Self::MSGQUEUE => Some(nix::libc::RLIMIT_MSGQUEUE as _),
            #[cfg(any(target_os = "linux", target_os = "android"))]
            Self::NICE => Some(nix::libc::RLIMIT_NICE as _),
            Self::NOFILE => Some(nix::libc::RLIMIT_NOFILE as _),
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
            Self::NPROC => Some(nix::libc::RLIMIT_NPROC as _),
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
            Self::RSS => Some(nix::libc::RLIMIT_RSS as _),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "emscripten"))]
            Self::RTPRIO => Some(nix::libc::RLIMIT_RTPRIO as _),
            #[cfg(target_os = "linux")]
            Self::RTTIME => Some(nix::libc::RLIMIT_RTTIME as _),
            #[cfg(any(target_os = "freebsd", target_os = "dragonfly", target_os = "netbsd"))]
            Self::SBSIZE => Some(nix::libc::RLIMIT_SBSIZE as _),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "emscripten"))]
            Self::SIGPENDING => Some(nix::libc::RLIMIT_SIGPENDING as _),
            Self::STACK => Some(nix::libc::RLIMIT_STACK as _),
            #[cfg(target_os = "freebsd")]
            Self::SWAP => Some(nix::libc::RLIMIT_SWAP as _),
            _ => None,
        }
    }
}

/// Pair of soft and hard limits
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LimitPair {
    pub soft: Limit,
    pub hard: Limit,
}

impl LimitPair {
    /// Returns `true` if the soft limit exceeds the hard limit
    #[must_use]
    pub fn soft_exceeds_hard(&self) -> bool {
        self.hard != INFINITY && (self.soft == INFINITY || self.soft > self.hard)
    }
}
