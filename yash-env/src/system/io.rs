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

//! Items about I/O operations

use super::Result;
use super::fd_flag::FdFlag;
use super::file_system::OfdAccess;
use crate::io::Fd;
use enumset::EnumSet;

/// Trait for closing file descriptors
pub trait Close {
    /// Closes a file descriptor.
    ///
    /// This is a thin wrapper around the [`close` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/close.html).
    ///
    /// If successful, returns `Ok(())`. On error, returns `Err(_)`.
    /// This function returns `Ok(())` when the FD is already closed, which is
    /// different from the behavior of the underlying system call.
    fn close(&self, fd: Fd) -> Result<()>;
}

/// Trait for creating pipes
///
/// This trait declares the `pipe` method, which creates an unnamed pipe. This
/// is a wrapper around the `pipe` system call.
pub trait Pipe {
    /// Creates an unnamed pipe.
    ///
    /// This is a thin wrapper around the [`pipe` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/pipe.html).
    /// If successful, returns the reading and writing ends of the pipe.
    fn pipe(&self) -> Result<(Fd, Fd)>;
}

/// Trait for duplicating file descriptors
///
/// This trait declares the `dup` and `dup2` methods, which duplicate file
/// descriptors.
pub trait Dup {
    /// Duplicates a file descriptor.
    ///
    /// This is a thin wrapper around the [`fcntl` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fcntl.html)
    /// that opens a new FD that shares the open file description with `from`.
    /// The new FD will be the minimum unused FD not less than `to_min`. The
    /// `flags` are set to the new FD.
    ///
    /// If successful, returns `Ok(new_fd)`. On error, returns `Err(_)`.
    fn dup(&self, from: Fd, to_min: Fd, flags: EnumSet<FdFlag>) -> Result<Fd>;

    /// Duplicates a file descriptor.
    ///
    /// This is a thin wrapper around the [`dup2` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/dup.html).
    /// If successful, returns `Ok(to)`. On error, returns `Err(_)`.
    fn dup2(&self, from: Fd, to: Fd) -> Result<Fd>;
}

/// Trait for `fcntl`-related operations
///
/// This trait declares methods related to the [`fcntl` system
/// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fcntl.html)
/// for manipulating file descriptors and open file descriptions.
pub trait Fcntl {
    /// Returns the open file description access mode.
    fn ofd_access(&self, fd: Fd) -> Result<OfdAccess>;

    /// Gets and sets the non-blocking mode for the open file description.
    ///
    /// This function sets the non-blocking mode to the given value and returns
    /// the previous mode.
    fn get_and_set_nonblocking(&self, fd: Fd, nonblocking: bool) -> Result<bool>;

    /// Returns the attributes for the file descriptor.
    fn fcntl_getfd(&self, fd: Fd) -> Result<EnumSet<FdFlag>>;

    /// Sets attributes for the file descriptor.
    fn fcntl_setfd(&self, fd: Fd, flags: EnumSet<FdFlag>) -> Result<()>;
}
