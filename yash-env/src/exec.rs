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

// TODO should be a struct
/// TODO describe
pub type ExitStatus = u32;

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
}

/// Result of command execution.
///
/// If the command was interrupted in the middle of execution, the result value
/// will be a [`Divert`] which specifies what to execute next.
pub type Result<T = ()> = std::result::Result<T, Divert>;
