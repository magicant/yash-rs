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

//! Defines flags for configuring open file descriptions.

use enumset::EnumSetType;

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
/// [`open`]: crate::system::System::open
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
