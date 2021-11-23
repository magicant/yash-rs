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

use nix::sys::signal::Signal;
use std::convert::TryFrom;
use std::ops::ControlFlow;
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

impl From<Signal> for ExitStatus {
    /// Converts a signal to the corresponding exit status.
    ///
    /// POSIX requires the exit status to be greater than 128. The current
    /// implementation returns `signal_number + 384`.
    fn from(signal: Signal) -> Self {
        Self::from(signal as c_int + 0x180)
    }
}

impl TryFrom<ExitStatus> for Signal {
    type Error = nix::Error;
    /// Converts an exit status to the corresponding signal.
    ///
    /// If there is a signal such that
    /// `exit_status == ExitStatus::from(signal)`,
    /// the signal is returned.
    /// The same if the exit status is the lowest 8 bits of such an exit status.
    /// The signal is also returned if the exit status is a signal number
    /// itself. Otherwise, an error is returned.
    fn try_from(exit_status: ExitStatus) -> nix::Result<Signal> {
        Signal::try_from(exit_status.0 - 0x180)
            .or_else(|_| Signal::try_from(exit_status.0 - 0x80))
            .or_else(|_| Signal::try_from(exit_status.0))
    }
}

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
///
/// `Divert` implements `Ord`. Values are ordered by severity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Divert {
    /// Continue the current loop.
    Continue {
        /// Number of loops to break before continuing.
        ///
        /// `0` for continuing the innermost loop, `1` for one-level outer, and so on.
        count: usize,
    },

    /// Break the current loop.
    Break {
        /// Number of loops to break.
        ///
        /// `0` for breaking the innermost loop, `1` for one-level outer, and so on.
        count: usize,
    },

    /// Return from the current function or script.
    Return,

    /// Interrupt the current shell execution environment.
    ///
    /// This is the same as `Exit` in a non-interactive shell. In an interactive
    /// shell, this will abort the currently executed command and resume
    /// prompting for a next command line.
    Interrupt(Option<ExitStatus>),

    /// Exit from the current shell execution environment.
    Exit(Option<ExitStatus>),
}

impl Divert {
    /// Returns the exit status associated with the `Divert`.
    ///
    /// Returns the variant's value if `self` is `Exit` or `Interrupt`;
    /// otherwise, `None`.
    pub fn exit_status(&self) -> Option<ExitStatus> {
        use Divert::*;
        match self {
            Continue { .. } | Break { .. } | Return => None,
            Interrupt(exit_status) | Exit(exit_status) => *exit_status,
        }
    }
}

/// Result of command execution.
///
/// If the command was interrupted in the middle of execution, the result value
/// will be a `Break` having a [`Divert`] value which specifies what to execute
/// next.
pub type Result<T = ()> = ControlFlow<Divert, T>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_try_from_exit_status() {
        let result = Signal::try_from(ExitStatus(0));
        assert!(result.is_err(), "{:?}", result);

        assert_eq!(
            Signal::try_from(ExitStatus(Signal::SIGINT as c_int)),
            Ok(Signal::SIGINT)
        );

        let mut exit_status = ExitStatus::from(Signal::SIGTERM);
        exit_status.0 &= 0xFF;
        assert_eq!(Signal::try_from(exit_status), Ok(Signal::SIGTERM));

        assert_eq!(
            Signal::try_from(ExitStatus::from(Signal::SIGHUP)),
            Ok(Signal::SIGHUP)
        );
    }
}
