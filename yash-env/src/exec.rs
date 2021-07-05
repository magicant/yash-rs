// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Type definitions for command execution.

use std::os::raw::c_int;

/// Number that summarizes the result of command execution.
///
/// An exit status is an integer returned from a utility (or command) when
/// executed. It usually is a summarized result of the execution.  Many
/// utilities return an exit status of zero when successful and non-zero
/// otherwise.
///
/// In the shell language, the special parameter `$?` expands to the exit status
/// of the last executed command. Exit statuses also affect the behavior of some
/// compound commands.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ExitStatus(pub c_int);

impl std::fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<c_int> for ExitStatus {
    fn from(value: c_int) -> ExitStatus {
        ExitStatus(value)
    }
}

impl From<ExitStatus> for c_int {
    fn from(exit_status: ExitStatus) -> c_int {
        exit_status.0
    }
}

// TODO Convert between ExitStatus and signal number

impl ExitStatus {
    /// Exit status of 0: success.
    pub const SUCCESS: ExitStatus = ExitStatus(0);

    /// Exit status of 1: failure.
    pub const FAILURE: ExitStatus = ExitStatus(1);

    /// Exit status of 2: error severer than failure.
    pub const ERROR: ExitStatus = ExitStatus(2);

    /// Exit Status of 126: command not executable.
    pub const NOEXEC: ExitStatus = ExitStatus(126);

    /// Exit status of 127: command not found.
    pub const NOT_FOUND: ExitStatus = ExitStatus(127);

    /// Returns true if and only if `self` is zero.
    pub const fn is_successful(&self) -> bool {
        self.0 == 0
    }
}

/// Result of interrupted command execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Divert {
    /// Break the current loop.
    Break {
        /// Number of loops to break.
        ///
        /// `0` for breaking the innermost loop, `1` for one-level outer, and so on.
        count: usize,
    },
    /// Continue the current loop.
    Continue,
    /// Return from the current function or script.
    Return,
    /// Exit from the current shell execution environment.
    Exit(ExitStatus),
}

/// Result of command execution.
///
/// If the command was interrupted in the middle of execution, the result value
/// will be a [`Divert`] which specifies what to execute next.
pub type Result<T = ()> = std::result::Result<T, Divert>;
