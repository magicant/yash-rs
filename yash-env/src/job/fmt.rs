// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Job report formatting
//!
//! This module defines utilities to format a job as specified in the POSIX. The
//! format is notably used by the jobs built-in but also used in the automatic
//! job status report printed between commands in an interactive shell.
//!
//! The format includes the job number, an optional marker representing the
//! current or previous job, the optional process ID, the current state, and the
//! job name, in this order. An example of a formatted job is:
//!
//! ```text
//! [2] + 24437 Running              cat foo bar | grep baz
//! ```
//!
//! To format a job, you create an instance of [`Report`] and use the `Display`
//! trait's method (typically by using the `format!` macro or calling the
//! `to_string` method).
//!
//! ```
//! use yash_env::job::fmt::{Marker, Report, State};
//! let report = Report {
//!     number: 3,
//!     marker: Marker::None,
//!     pid: None,
//!     state: State::Running,
//!     name: "sleep 10",
//! };
//! assert_eq!(report.to_string(), "[3]   Running              sleep 10");
//! ```

#[cfg(doc)]
use super::JobList;
use super::Pid;
use super::ProcessResult;
use super::ProcessState;
use crate::semantics::ExitStatus;
use crate::signal;
use crate::system::System;
use crate::system::SystemEx;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result;

/// Type of a marker indicating the current and previous job
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Marker {
    None,
    CurrentJob,
    PreviousJob,
}

impl Marker {
    /// Returns the character representation of the marker.
    ///
    /// This function returns `' '`, `'+'`, and `'-'` for `None`, `CurrentJob`,
    /// and `PreviousJob`, respectively.
    pub const fn as_char(self) -> char {
        match self {
            Marker::None => ' ',
            Marker::CurrentJob => '+',
            Marker::PreviousJob => '-',
        }
    }
}

impl Display for Marker {
    fn fmt(&self, f: &mut Formatter) -> Result {
        self.as_char().fmt(f)
    }
}

/// State of a job
///
/// This enumeration represents the state of a job. The string representation of
/// the state is used in the job status report.
///
/// This type is similar to the [`ProcessState`] type, but it uses
/// [`signal::Name`] instead of [`signal::Number`] to represent signals so that
/// the signal names are shown in the job status report.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum State {
    /// The job is running.
    Running,
    /// The process has been stopped by a signal.
    Stopped(signal::Name),
    /// The process has exited.
    Exited(ExitStatus),
    /// The process has been terminated by a signal.
    Signaled {
        signal: signal::Name,
        core_dump: bool,
    },
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Self::Running => "Running".fmt(f),

            Self::Exited(ExitStatus::SUCCESS) => "Done".fmt(f),

            Self::Exited(exit_status) => format!("Done({exit_status})").fmt(f),

            Self::Stopped(signal) => format!("Stopped(SIG{signal})").fmt(f),

            Self::Signaled {
                signal,
                core_dump: false,
            } => format!("Killed(SIG{signal})").fmt(f),

            Self::Signaled {
                signal,
                core_dump: true,
            } => format!("Killed(SIG{signal}: core dumped)").fmt(f),
        }
    }
}

impl State {
    /// Creates a `State` from a process state.
    ///
    /// This function converts a [`ProcessState`] into a `State` by converting
    /// the contained signal number into a signal name using the given system.
    /// If the signal number is not recognized, the signal name `Rtmin(-1)` is
    /// used as a fallback replacement.
    #[must_use]
    pub fn from_process_state<S: System>(state: ProcessState, system: &S) -> Self {
        match state {
            ProcessState::Running => Self::Running,
            ProcessState::Halted(result) => match result {
                ProcessResult::Exited(status) => Self::Exited(status),
                ProcessResult::Stopped(signal) => {
                    Self::Stopped(system.signal_name_from_number(signal))
                }
                ProcessResult::Signaled { signal, core_dump } => {
                    let signal = system.signal_name_from_number(signal);
                    Self::Signaled { signal, core_dump }
                }
            },
        }
    }
}

/// Set of data to produce a job status report
///
/// This structure contains the information necessary to format a job status
/// report, which can be produced by the `Display` trait's method.
/// See the [module documentation](self) for details.
#[derive(Clone, Debug)]
pub struct Report<'a> {
    /// Job number
    ///
    /// Usually, this value is the index of the job in the job list plus one.
    pub number: usize,

    /// Type of the marker indicating the current and previous job
    pub marker: Marker,

    /// Process ID of the job
    ///
    /// This field is optional and is only included when it is not `None`.
    pub pid: Option<Pid>,

    /// Current state of the job
    pub state: State,

    /// Job name
    pub name: &'a str,
}

impl std::fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "[{}] {} ", self.number, self.marker)?;
        if let Some(pid) = self.pid {
            write!(f, "{:5} ", pid)?;
        }
        write!(f, "{:20} {}", self.state, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Pid;
    use super::*;

    #[test]
    fn state_display() {
        let state = State::Running;
        assert_eq!(state.to_string(), "Running");
        let state = State::Stopped(signal::Name::Stop);
        assert_eq!(state.to_string(), "Stopped(SIGSTOP)");
        let state = State::Exited(ExitStatus::SUCCESS);
        assert_eq!(state.to_string(), "Done");
        let state = State::Exited(ExitStatus::NOT_FOUND);
        assert_eq!(state.to_string(), "Done(127)");
        let state = State::Signaled {
            signal: signal::Name::Kill,
            core_dump: false,
        };
        assert_eq!(state.to_string(), "Killed(SIGKILL)");
        let state = State::Signaled {
            signal: signal::Name::Quit,
            core_dump: true,
        };
        assert_eq!(state.to_string(), "Killed(SIGQUIT: core dumped)");
    }

    #[test]
    fn report_display() {
        let mut report = Report {
            number: 1,
            marker: Marker::None,
            pid: None,
            state: State::Running,
            name: "echo ok",
        };
        assert_eq!(report.to_string(), "[1]   Running              echo ok");

        report.number = 2;
        assert_eq!(report.to_string(), "[2]   Running              echo ok");

        report.marker = Marker::CurrentJob;
        assert_eq!(report.to_string(), "[2] + Running              echo ok");

        report.pid = Some(Pid(42));
        assert_eq!(
            report.to_string(),
            "[2] +    42 Running              echo ok"
        );

        report.pid = Some(Pid(123456));
        assert_eq!(
            report.to_string(),
            "[2] + 123456 Running              echo ok"
        );

        report.name = "foo | bar";
        assert_eq!(
            report.to_string(),
            "[2] + 123456 Running              foo | bar"
        );
    }
}
