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
//! The wait built-in waits for asynchronous jobs to finish.
//!
//! # Syntax
//!
//! ```sh
//! wait [job_id_or_process_id...]
//! ```
//!
//! # Options
//!
//! None
//!
//! # Operands
//!
//! An operand can be a job ID or decimal process ID, specifying which job to
//! wait for.
//!
//! TODO Elaborate on syntax of job ID
//!
//! If you don't specify any operand, the built-in waits for all existing
//! asynchronous jobs.
//!
//! # Exit status
//!
//! TBD
//!
//! # Errors
//!
//! TBD
//!
//! # Portability
//!
//! The wait built-in is contained in the POSIX standard.

use std::future::Future;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::Env;

// TODO Wait for all jobs if there is no operand
// TODO Wait for jobs specified by operands
// TODO Parse as a job ID if an operand starts with %
// TODO Treat an unknown job as terminated with exit status 127
// TODO Treat a suspended job as terminated if it is job-controlled.
// TODO Interruption by trap
// TODO Allow interrupting with SIGINT if interactive

/// Implementation of the wait built-in.
pub async fn builtin_body(_env: &mut Env, _args: Vec<Field>) -> Result {
    (ExitStatus::SUCCESS, Continue(()))
}

/// Wrapper of [`builtin_body`] that returns the future in a pinned box.
pub fn builtin_main(
    env: &mut yash_env::Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result> + '_>> {
    Box::pin(builtin_body(env, args))
}
