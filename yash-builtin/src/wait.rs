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

//! Wait built-in
//!
//! The **`wait`** built-in waits for asynchronous jobs to finish.
//!
//! # Synopsis
//!
//! ```sh
//! wait [job_id_or_process_id…]
//! ```
//!
//! # Description
//!
//! If you specify one or more operands, the built-in waits for the specified
//! job to finish. Otherwise, the built-in waits for all existing asynchronous
//! jobs.
//!
//! If the job is already finished, the built-in returns without waiting. If the
//! job is job-controlled (that is, running in its own process group), it is
//! considered finished not only when it has exited but also when it has been
//! suspended.
//!
//! # Options
//!
//! None
//!
//! # Operands
//!
//! An operand can be a job ID or decimal process ID, specifying which job to
//! wait for. A job ID must start with `%` and has the format described in the
//! [`yash_env::job::id`] module documentation. A process ID is a non-negative
//! decimal integer.
//!
//! If there is no job matching the operand, the built-in assumes that the
//! job has already finished with exit status 127.
//!
//! # Errors
//!
//! It is an error if an operand is not a job ID or decimal process ID.
//!
//! It is an error if a job ID matches more than one job.
//!
//! # Exit status
//!
//! If you specify one or more operands, the built-in returns the exit status of
//! the job specified by the last operand. If there is no operand, the exit
//! status is 0 regardless of the awaited jobs.
//!
//! If the built-in was interrupted by a signal, the exit status indicates the
//! signal.
//!
//! The exit status is between 1 and 126 (inclusive) for any other error.
//!
//! # Portability
//!
//! The wait built-in is contained in the POSIX standard.
//!
//! The exact value of an exit status resulting from a signal is
//! implementation-dependent.
//!
//! Many existing shells behave differently on various errors. POSIX requires
//! that an unknown process ID be treated as a process that has already exited
//! with exit status 127, but the behavior for other errors should not be
//! considered portable.

use yash_env::job::Pid;
use yash_env::semantics::Field;
use yash_env::Env;

mod old;

/// Job specification (job ID or process ID)
///
/// Each operand of the `wait` built-in is parsed into a `JobSpec` value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobSpec {
    /// Process ID (non-negative decimal integer)
    ProcessId(Pid),

    /// Job ID (string of the form `%…`)
    JobId(Field),
}

/// Parsed command line arguments to the `wait` built-in
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    /// Operands that specify which jobs to wait for
    ///
    /// If empty, the built-in waits for all existing asynchronous jobs.
    pub jobs: Vec<JobSpec>,
}

pub mod search;
pub mod syntax;

/// Entry point for executing the `wait` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    old::main(env, args).await
}
