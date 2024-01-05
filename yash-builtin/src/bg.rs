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

//! Bg built-in
//!
//! The **`bg`** built-in resumes a suspended job in the background.
//!
//! # Synopsis
//!
//! ```sh
//! bg [job_id…]
//! ```
//!
//! # Description
//!
//! The built-in resumes the specified jobs by sending the `SIGCONT` signal to
//! them.
//!
//! # Options
//!
//! None.
//!
//! # Operands
//!
//! Operands specify which jobs to resume. See the module documentation of
//! [`yash_env::job::id`] for the format of job IDs. If omitted, the built-in
//! resumes the [current job](JobSet::current_job).
//!
//! (TODO: allow omitting the leading `%`)
//!
//! # Standard output
//!
//! The built-in writes the job number and name of each resumed job to the
//! standard output.
//!
//! # Errors
//!
//! This built-in can be used only when the [`Monitor`] option is set.
//!
//! It is an error if the specified job is not found.
//!
//! # Exit status
//!
//! Zero unless an error occurs.
//!
//! # Portability
//!
//! Many implementations allow omitting the leading `%` from job IDs, though it
//! is not required by POSIX.
//!
//! # Implementation notes
//!
//! This implementation sends the `SIGCONT` signal even to jobs that are already
//! running.
//!
//! [`Monitor`]: yash_env::option::Monitor

#[cfg(doc)]
use yash_env::job::JobSet;
use yash_env::semantics::Field;
use yash_env::Env;

/// Entry point of the `bg` built-in
pub async fn main(_env: &mut Env, _args: Vec<Field>) -> crate::Result {
    todo!()
}
