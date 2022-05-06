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
//! TODO Code example

use super::Job;
use super::WaitStatus;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result;

/// Wrapper for formatting `WaitStatus`
///
/// This type is a thin wrapper of `WaitStatus` that implements the `Display`
/// trait to format the wait status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FormatStatus(WaitStatus);

impl Display for FormatStatus {
    fn fmt(&self, f: &mut Formatter) -> Result {
        let s = match self.0 {
            WaitStatus::Stopped(_, signal) => format!("Stopped({signal})"),
            WaitStatus::Exited(_, 0) => return f.pad("Done"),
            WaitStatus::Exited(_, exit_status) => format!("Done({exit_status})"),
            WaitStatus::Signaled(_, signal, false) => format!("Killed({signal})"),
            WaitStatus::Signaled(_, signal, true) => format!("Killed({signal}: core dumped)"),
            _ => return f.pad("Running"),
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

/// Formats a job status report.
///
/// The `fmt` method will **panic** if `self.index` does not name an existing
/// job in `self.jobs`.
impl Display for Report<'_> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        let number = self.index + 1;
        let marker = self.marker;
        let status = FormatStatus(self.job.status);
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
    use crate::trap::Signal;

    #[test]
    fn format_status_running() {
        let fs = FormatStatus(WaitStatus::StillAlive);
        assert_eq!(fs.to_string(), "Running");
        let fs = FormatStatus(WaitStatus::Continued(Pid::from_raw(0)));
        assert_eq!(fs.to_string(), "Running");
    }

    #[test]
    fn format_status_stopped() {
        let fs = FormatStatus(WaitStatus::Stopped(Pid::from_raw(0), Signal::SIGSTOP));
        assert_eq!(fs.to_string(), "Stopped(SIGSTOP)");
        let fs = FormatStatus(WaitStatus::Stopped(Pid::from_raw(1), Signal::SIGTSTP));
        assert_eq!(fs.to_string(), "Stopped(SIGTSTP)");
        let fs = FormatStatus(WaitStatus::Stopped(Pid::from_raw(2), Signal::SIGTTIN));
        assert_eq!(fs.to_string(), "Stopped(SIGTTIN)");
        let fs = FormatStatus(WaitStatus::Stopped(Pid::from_raw(2), Signal::SIGTTOU));
        assert_eq!(fs.to_string(), "Stopped(SIGTTOU)");
    }

    #[test]
    fn format_status_exited() {
        let fs = FormatStatus(WaitStatus::Exited(Pid::from_raw(10), 0));
        assert_eq!(fs.to_string(), "Done");
        let fs = FormatStatus(WaitStatus::Exited(Pid::from_raw(11), 1));
        assert_eq!(fs.to_string(), "Done(1)");
        let fs = FormatStatus(WaitStatus::Exited(Pid::from_raw(12), 2));
        assert_eq!(fs.to_string(), "Done(2)");
        let fs = FormatStatus(WaitStatus::Exited(Pid::from_raw(12), 253));
        assert_eq!(fs.to_string(), "Done(253)");
    }

    #[test]
    fn format_status_signaled() {
        let fs = FormatStatus(WaitStatus::Signaled(
            Pid::from_raw(0),
            Signal::SIGKILL,
            false,
        ));
        assert_eq!(fs.to_string(), "Killed(SIGKILL)");

        let fs = FormatStatus(WaitStatus::Signaled(
            Pid::from_raw(10),
            Signal::SIGKILL,
            true,
        ));
        assert_eq!(fs.to_string(), "Killed(SIGKILL: core dumped)");

        let fs = FormatStatus(WaitStatus::Signaled(
            Pid::from_raw(20),
            Signal::SIGTERM,
            false,
        ));
        assert_eq!(fs.to_string(), "Killed(SIGTERM)");

        let fs = FormatStatus(WaitStatus::Signaled(
            Pid::from_raw(30),
            Signal::SIGQUIT,
            true,
        ));
        assert_eq!(fs.to_string(), "Killed(SIGQUIT: core dumped)");
    }

    #[test]
    fn report_standard() {
        let index = 0;
        let marker = Marker::CurrentJob;
        let job = &mut Job::new(Pid::from_raw(42));
        job.name = "echo ok".to_string();
        let report = Report { index, marker, job };
        assert_eq!(report.to_string(), "[1] + Running              echo ok");

        job.status = WaitStatus::Stopped(Pid::from_raw(42), Signal::SIGSTOP);
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

        job.status = WaitStatus::Signaled(Pid::from_raw(16), Signal::SIGQUIT, true);
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
