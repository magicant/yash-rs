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
//! current or previous job, the current status, and the job name, in this
//! order. An example of a formatted job is:
//!
//! ```text
//! [2] + Running              cat foo bar | grep baz
//! ```
//!
//! The process ID is inserted before the current status when the alternate mode
//! flag (`#`) is used:
//!
//! ```text
//! [2] + 24437 Running              cat foo bar | grep baz
//! ```
//!
//! To format a job, you create an instance of [`Report`] and use the `Display`
//! trait's method (typically by using the `format!` macro).
//!
//! ```
//! use yash_env::job::{Job, Pid};
//! use yash_env::job::fmt::{Marker, Report};
//! let mut job = Job::new(Pid::from_raw(123));
//! job.name = "sleep 10".to_string();
//! let report = Report {
//!     index: 2,
//!     marker: Marker::None,
//!     job: &job,
//! };
//! let s = format!("{}", report);
//! assert_eq!(s, "[3]   Running              sleep 10");
//! let s = format!("{:#}", report);
//! assert_eq!(s, "[3]     123 Running              sleep 10");
//! ```

use super::Job;
use super::ProcessState;
use crate::semantics::ExitStatus;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result;

/// Formats a process state into a string.
///
/// Process states are formatted as follows:
///
/// - `Running` for a running process
/// - `Stopped(SIG…)` for a stopped process that has been stopped by the signal
///   `SIG…`
/// - `Done` for a process that exited with exit status 0
/// - `Done(…)` for a process that exited with a non-zero exit status where
///   `…` is the exit status
/// - `Killed(SIG…)` for a process that was terminated by the signal `SIG…`
///   without a core dump
/// - `Killed(SIG…: core dumped)` for a process that was terminated by the
///   signal `SIG…` with a core dump
impl Display for ProcessState {
    fn fmt(&self, f: &mut Formatter) -> Result {
        let s = match self {
            ProcessState::Running => return f.pad("Running"),
            ProcessState::Stopped(signal) => format!("Stopped({signal})"),
            ProcessState::Exited(ExitStatus(0)) => return f.pad("Done"),
            ProcessState::Exited(exit_status) => format!("Done({exit_status})"),
            ProcessState::Signaled {
                signal,
                core_dump: false,
            } => format!("Killed({signal})"),
            ProcessState::Signaled {
                signal,
                core_dump: true,
            } => format!("Killed({signal}: core dumped)"),
        };
        f.pad(&s)
    }
}

/// Type of a marker indicating the current and previous job
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

/// Wrapper for implementing job status formatting
///
/// This type is a thin wrapper of a job that implements the `Display` trait to
/// format a job status report. See the [module documentation](self) for details.
#[derive(Clone, Copy, Debug)]
pub struct Report<'a> {
    /// Index of the job
    ///
    /// This value should be the index at which the job appears in its
    /// containing job set.
    ///
    /// Note that the index is the job number minus one.
    pub index: usize,

    /// Type of the marker indicating the current and previous job
    pub marker: Marker,

    /// Job to be reported
    pub job: &'a Job,
}

impl Report<'_> {
    /// Returns the job number of the job.
    ///
    /// The job number is a positive integer that is one greater than the index
    /// of the job in its containing job set. Rather than the raw index, the job
    /// number should be displayed to the user.
    #[must_use]
    pub fn number(&self) -> usize {
        self.index + 1
    }
}

/// Formats a job status report.
///
/// The `fmt` method will **panic** if `self.index` does not name an existing
/// job in `self.jobs`.
impl Display for Report<'_> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        let number = self.number();
        let marker = self.marker;
        let status = self.job.state;
        let name = &self.job.name;
        if f.alternate() {
            let pid = self.job.pid;
            write!(f, "[{number}] {marker} {pid:5} {status:20} {name}")
        } else {
            write!(f, "[{number}] {marker} {status:20} {name}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Job;
    use super::super::Pid;
    use super::*;
    use crate::job::ProcessState;
    use crate::trap::Signal;

    #[test]
    fn process_state_display_running() {
        let state = ProcessState::Running;
        assert_eq!(state.to_string(), "Running");
    }

    #[test]
    fn process_state_display_stopped() {
        let state = ProcessState::Stopped(Signal::SIGSTOP);
        assert_eq!(state.to_string(), "Stopped(SIGSTOP)");
        let state = ProcessState::Stopped(Signal::SIGTSTP);
        assert_eq!(state.to_string(), "Stopped(SIGTSTP)");
        let state = ProcessState::Stopped(Signal::SIGTTIN);
        assert_eq!(state.to_string(), "Stopped(SIGTTIN)");
        let state = ProcessState::Stopped(Signal::SIGTTOU);
        assert_eq!(state.to_string(), "Stopped(SIGTTOU)");
    }

    #[test]
    fn process_state_display_exited() {
        let state = ProcessState::Exited(ExitStatus(0));
        assert_eq!(state.to_string(), "Done");
        let state = ProcessState::Exited(ExitStatus(1));
        assert_eq!(state.to_string(), "Done(1)");
        let state = ProcessState::Exited(ExitStatus(2));
        assert_eq!(state.to_string(), "Done(2)");
        let state = ProcessState::Exited(ExitStatus(253));
        assert_eq!(state.to_string(), "Done(253)");
    }

    #[test]
    fn process_state_display_signaled() {
        let state = ProcessState::Signaled {
            signal: Signal::SIGKILL,
            core_dump: false,
        };
        assert_eq!(state.to_string(), "Killed(SIGKILL)");

        let state = ProcessState::Signaled {
            signal: Signal::SIGKILL,
            core_dump: true,
        };
        assert_eq!(state.to_string(), "Killed(SIGKILL: core dumped)");

        let state = ProcessState::Signaled {
            signal: Signal::SIGTERM,
            core_dump: false,
        };
        assert_eq!(state.to_string(), "Killed(SIGTERM)");

        let state = ProcessState::Signaled {
            signal: Signal::SIGQUIT,
            core_dump: true,
        };
        assert_eq!(state.to_string(), "Killed(SIGQUIT: core dumped)");
    }

    #[test]
    fn report_standard() {
        let index = 0;
        let marker = Marker::CurrentJob;
        let job = &mut Job::new(Pid::from_raw(42));
        job.name = "echo ok".to_string();
        let report = Report { index, marker, job };
        assert_eq!(report.to_string(), "[1] + Running              echo ok");

        job.state = ProcessState::Stopped(Signal::SIGSTOP);
        let report = Report { index, marker, job };
        assert_eq!(report.to_string(), "[1] + Stopped(SIGSTOP)     echo ok");

        let marker = Marker::PreviousJob;
        let report = Report { index, marker, job };
        assert_eq!(report.to_string(), "[1] - Stopped(SIGSTOP)     echo ok");

        let marker = Marker::None;
        let report = Report { index, marker, job };
        assert_eq!(report.to_string(), "[1]   Stopped(SIGSTOP)     echo ok");

        let index = 5;
        let report = Report { index, marker, job };
        assert_eq!(report.to_string(), "[6]   Stopped(SIGSTOP)     echo ok");

        job.state = ProcessState::Signaled {
            signal: Signal::SIGQUIT,
            core_dump: true,
        };
        job.name = "exit 0".to_string();
        let report = Report { index, marker, job };
        assert_eq!(
            report.to_string(),
            "[6]   Killed(SIGQUIT: core dumped) exit 0"
        );
    }

    #[test]
    fn report_alternate() {
        let index = 0;
        let marker = Marker::CurrentJob;
        let job = &mut Job::new(Pid::from_raw(42));
        job.name = "echo ok".to_string();
        let report = Report { index, marker, job };
        assert_eq!(
            format!("{report:#}"),
            "[1] +    42 Running              echo ok"
        );

        job.pid = Pid::from_raw(123456);
        let report = Report { index, marker, job };
        assert_eq!(
            format!("{report:#}"),
            "[1] + 123456 Running              echo ok"
        );
    }
}
