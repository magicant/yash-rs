// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Fg built-in
//!
//! The **`fg`** resumes a suspended job in the foreground.
//!
//! # Synopsis
//!
//! ```sh
//! fg [job_id]
//! ```
//!
//! # Description
//!
//! The built-in brings the specified job to the foreground and resumes its
//! execution by sending the `SIGCONT` signal to it. The built-in then waits for
//! the job to finish (or suspend again).
//!
//! If the job gets suspended again, it is set as the [current
//! job](JobSet::current_job).
//!
//! # Options
//!
//! None.
//!
//! # Operands
//!
//! Operand *job_id* specifies which job to resume. See the module documentation
//! of [`yash_env::job::id`] for the format of job IDs. If omitted, the built-in
//! resumes the [current job](JobSet::current_job).
//!
//! (TODO: allow omitting the leading `%`)
//! (TODO: allow multiple operands)
//!
//! # Standard output
//!
//! The built-in writes the selected job name to the standard output.
//!
//! (TODO: print the job number as well)
//!
//! # Errors
//!
//! This built-in can be used only when the [`Monitor`] option is set.
//!
//! It is an error if the specified job is not found.
//!
//! # Exit status
//!
//! The built-in returns the exit status of the resumed job. On error, it
//! returns a non-zero exit status.
//!
//! # Portability
//!
//! Many implementations allow omitting the leading `%` from job IDs and
//! specifying multiple job IDs at once, though this is not required by POSIX.
//!
//! # Implementation notes
//!
//! This implementation sends the `SIGCONT` signal even if the job is already
//! running.
//!
//! [`Monitor`]: yash_env::option::Monitor

#[cfg(doc)]
use yash_env::job::JobSet;
use yash_env::semantics::Field;
use yash_env::Env;

/// Entry point of the `fg` built-in
pub async fn main(_env: &mut Env, _args: Vec<Field>) -> crate::Result {
    todo!()
}
