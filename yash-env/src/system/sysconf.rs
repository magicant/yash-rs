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

//! Items for obtaining system configuration information

use super::Result;
use crate::str::UnixString;
use std::ffi::CString;

/// Trait for getting system configuration information
pub trait Sysconf {
    /// Returns the standard `$PATH` value where all standard utilities are
    /// expected to be found.
    ///
    /// This is a thin wrapper around `confstr(_CS_PATH, …)`.
    fn confstr_path(&self) -> Result<UnixString>;
}

pub trait ShellPath {
    // TODO: Should return `Cow<CStr>` instead
    /// Returns the path to the shell executable.
    ///
    /// If possible, this function should return the path to the current shell
    /// executable. Otherwise, it should return the path to the default POSIX
    /// shell.
    fn shell_path(&self) -> CString;
}
