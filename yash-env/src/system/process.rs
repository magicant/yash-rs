// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! Items related to process management

use super::Result;
use crate::job::Pid;

/// Trait for getting the current process ID and other process-related information
pub trait GetPid {
    /// Returns the process ID of the current process.
    ///
    /// This method represents the [`getpid` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/getpid.html).
    #[must_use]
    fn getpid(&self) -> Pid;

    /// Returns the process ID of the parent process.
    ///
    /// This method represents the [`getppid` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/getppid.html).
    #[must_use]
    fn getppid(&self) -> Pid;

    /// Returns the process group ID of the current process.
    ///
    /// This method represents the [`getpgrp` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/getpgrp.html).
    #[must_use]
    fn getpgrp(&self) -> Pid;

    /// Returns the session ID of the specified process.
    ///
    /// If `pid` is `Pid(0)`, this function returns the session ID of the
    /// current process.
    fn getsid(&self, pid: Pid) -> Result<Pid>;
}

/// Trait for modifying the process group ID of processes
pub trait SetPgid {
    /// Modifies the process group ID of a process.
    ///
    /// This method represents the [`setpgid` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/setpgid.html).
    ///
    /// `pid` specifies the process whose process group ID is to be changed. If `pid` is
    /// `Pid(0)`, the current process is used.
    /// `pgid` specifies the new process group ID to be set. If `pgid` is
    /// `Pid(0)`, the process ID of the specified process is used.
    fn setpgid(&self, pid: Pid, pgid: Pid) -> Result<()>;
}
