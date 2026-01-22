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
//!
//! When you have multiple jobs to report, you can use the [`Accumulator`] to
//! collect the reports and then print them as a single string.

use super::Job;
#[cfg(doc)]
use super::JobList;
use super::Pid;
use super::ProcessResult;
use super::ProcessState;
use crate::semantics::ExitStatus;
use crate::signal;
use crate::system::Signals;
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

            Self::Exited(exit_status) => write!(f, "Done({exit_status})"),

            Self::Stopped(signal) => write!(f, "Stopped(SIG{signal})"),

            Self::Signaled {
                signal,
                core_dump: false,
            } => write!(f, "Killed(SIG{signal})"),

            Self::Signaled {
                signal,
                core_dump: true,
            } => write!(f, "Killed(SIG{signal}: core dumped)"),
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
    pub fn from_process_state<S: Signals>(state: ProcessState, system: &S) -> Self {
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
            write!(f, "{pid:5} ")?;
        }
        write!(f, "{:20} {}", self.state, self.name)
    }
}

/// Intermediate data to construct a report for multiple jobs
///
/// This structure contains parameters that modify the behavior of the job
/// report formatting and a string buffer to accumulate the formatted reports.
/// After constructing the accumulator, you can use it to collect multiple
/// reports and then print them as a single string. The accumulator also
/// remembers the indices of the jobs that have been reported, so that you can
/// clear the `status_changed` flags of the jobs after printing the reports to
/// avoid printing the same report multiple times.
///
/// ## Examples
///
/// ```
/// use yash_env::job::{Job, Pid, ProcessState};
/// use yash_env::job::fmt::Accumulator;
/// use yash_env::system::r#virtual::{SIGSTOP, VirtualSystem};
///
/// let system = VirtualSystem::new();
/// let mut accumulator = Accumulator::new();
/// accumulator.current_job_index = Some(3);
/// accumulator.previous_job_index = Some(0);
///
/// let mut job1 = Job::new(Pid(10));
/// job1.state = ProcessState::Running;
/// job1.name = "echo foo".to_string();
/// accumulator.add(0, &job1, &system);
/// let mut job2 = Job::new(Pid(20));
/// job2.state = ProcessState::exited(0);
/// job2.name = "echo bar".to_string();
/// accumulator.add(2, &job2, &system);
/// let mut job3 = Job::new(Pid(30));
/// job3.state = ProcessState::stopped(SIGSTOP);
/// job3.name = "echo baz".to_string();
/// accumulator.add(3, &job3, &system);
///
/// assert_eq!(
///     accumulator.print,
///     "[1] - Running              echo foo\n\
///      [3]   Done                 echo bar\n\
///      [4] + Stopped(SIGSTOP)     echo baz\n"
/// );
/// assert_eq!(accumulator.indices_reported, [0, 2, 3]);
/// ```
#[derive(Clone, Debug, Default)]
pub struct Accumulator {
    /// Index of the current job in the job list
    pub current_job_index: Option<usize>,
    /// Index of the previous job in the job list
    pub previous_job_index: Option<usize>,
    /// Whether to show the process ID in the report
    pub show_pid: bool,
    /// Whether to show only the process group ID in the report
    pub pgid_only: bool,
    /// Accumulated reports formatted as a single string
    pub print: String,
    /// Indices of the jobs that have been reported
    pub indices_reported: Vec<usize>,
}

impl Accumulator {
    /// Creates a new `Accumulator` with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a job to the accumulator.
    ///
    /// This function formats the given job as a [`Report`] and appends it to
    /// the accumulated reports in the `print` field. The `index` parameter is
    /// the index of the job in the job list. The `system` parameter is used to
    /// convert the process state into a job [`State`].
    ///
    /// The `indices_reported` field is updated to include the `index`
    /// parameter.
    pub fn add<S: Signals>(&mut self, index: usize, job: &Job, system: &S) {
        use std::fmt::Write as _;

        if self.pgid_only {
            writeln!(self.print, "{}", job.pid)
        } else {
            let report = Report {
                number: index + 1,
                marker: if self.current_job_index == Some(index) {
                    Marker::CurrentJob
                } else if self.previous_job_index == Some(index) {
                    Marker::PreviousJob
                } else {
                    Marker::None
                },
                pid: self.show_pid.then_some(job.pid),
                state: State::from_process_state(job.state, system),
                name: &job.name,
            };
            writeln!(self.print, "{report}")
        }
        .unwrap();

        self.indices_reported.push(index);
    }
}

#[cfg(test)]
mod tests {
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
