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

//! Items for controlling terminal devices

use super::{Fd, Pid, Result};
use std::future::Future;

// TODO: Isatty should be a subtrait of TcGetAttr
/// Trait for testing if a file descriptor is associated with a terminal device
///
/// This trait declares the `isatty` method, which tests whether a file
/// descriptor is associated with a terminal device.
pub trait Isatty {
    /// Tests if a file descriptor is associated with a terminal device.
    ///
    /// On error, this function simply returns `false` and no detailed error
    /// information is provided because POSIX does not require the `isatty`
    /// function to set `errno`.
    #[must_use]
    fn isatty(&self, fd: Fd) -> bool;
}

/// Trait for getting the foreground process group ID of a terminal
pub trait TcGetPgrp {
    /// Returns the current foreground process group ID.
    ///
    /// If the terminal associated with the file descriptor `fd` has no
    /// foreground process group, the return value is an unused process group ID
    /// greater than 1.
    ///
    /// This is a thin wrapper around the [`tcgetpgrp` system
    /// function](https://pubs.opengroup.org/onlinepubs/9799919799/functions/tcgetpgrp.html).
    fn tcgetpgrp(&self, fd: Fd) -> Result<Pid>;
}

/// Trait for setting the foreground process group ID of a terminal
pub trait TcSetPgrp {
    /// Switches the foreground process group.
    ///
    /// This is a thin wrapper around the [`tcsetpgrp` system
    /// function](https://pubs.opengroup.org/onlinepubs/9799919799/functions/tcsetpgrp.html).
    ///
    /// The virtual system version of this function may block the calling thread
    /// if called in a background process group, hence returning a future.
    fn tcsetpgrp(&self, fd: Fd, pgid: Pid) -> impl Future<Output = Result<()>> + use<Self>;
}
