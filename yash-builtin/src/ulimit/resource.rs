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

//! Extension of [`Resource`] for the `ulimit` built-in

use yash_env::system::resource::{rlim_t, Resource};

/// Extension of [`Resource`] for use in the `ulimit` built-in
pub trait ResourceExt {
    /// Returns the option character for the resource.
    ///
    /// The returned character can be used in a short option to specify the
    /// resource. For example, the option character for [`Resource::AS`] is
    /// `v`.
    #[must_use]
    fn option(&self) -> char;

    /// Returns a human-readable description of the resource.
    ///
    /// The returned string is not localized.
    #[must_use]
    fn description(&self) -> &'static str;

    /// Returns the scale of the resource.
    ///
    /// The scale is the ratio of the actual limit to the value that the user
    /// sees and sets. For example, the scale of [`Resource::DATA`] is 1024,
    /// which means that the user sees and sets the limit in kilobytes, but the
    /// underlying system call operates in bytes.
    #[must_use]
    fn scale(&self) -> rlim_t;
}

impl ResourceExt for Resource {
    fn option(&self) -> char {
        match self {
            Self::AS => 'v',
            Self::CORE => 'c',
            Self::CPU => 't',
            Self::DATA => 'd',
            Self::FSIZE => 'f',
            Self::KQUEUES => 'k',
            Self::LOCKS => 'x',
            Self::MEMLOCK => 'l',
            Self::MSGQUEUE => 'q',
            Self::NICE => 'e',
            Self::NOFILE => 'n',
            Self::NPROC => 'u',
            Self::RSS => 'm',
            Self::RTPRIO => 'r',
            Self::RTTIME => 'R',
            Self::SBSIZE => 'b',
            Self::SIGPENDING => 'i',
            Self::STACK => 's',
            Self::SWAP => 'w',
            _ => '\0',
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::AS => "virtual address space size (KiB)",
            Self::CORE => "core dump size (512-byte blocks)",
            Self::CPU => "CPU time (seconds)",
            Self::DATA => "data segment size (KiB)",
            Self::FSIZE => "file size (512-byte blocks)",
            Self::KQUEUES => "number of kqueues",
            Self::LOCKS => "number of file locks",
            Self::MEMLOCK => "locked memory size (KiB)",
            Self::MSGQUEUE => "message queue size (bytes)",
            Self::NICE => "process priority (20 - nice)",
            Self::NOFILE => "number of open files",
            Self::NPROC => "number of processes",
            Self::RSS => "resident set size (KiB)",
            Self::RTPRIO => "real-time priority",
            Self::RTTIME => "real-time timeout (microseconds)",
            Self::SBSIZE => "socket buffer size (bytes)",
            Self::SIGPENDING => "number of pending signals",
            Self::STACK => "stack size (KiB)",
            Self::SWAP => "swap space size (KiB)",
            _ => "unknown resource",
        }
    }

    fn scale(&self) -> rlim_t {
        match self {
            Self::AS | Self::DATA | Self::MEMLOCK | Self::RSS | Self::STACK | Self::SWAP => 1 << 10,
            Self::CORE | Self::FSIZE => 1 << 9,
            _ => 1,
        }
    }
}
