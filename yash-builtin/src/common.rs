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

//! Common items for implementing built-ins
//!
//! This module contains some utility functions for printing messages and a
//! submodule for [parsing command line arguments](syntax).

use yash_env::Env;
use yash_env::io::Fd;
use yash_env::system::{Fcntl, Isatty, Write};

pub mod report;
pub mod syntax;

/// Prints a text to the standard output.
///
/// This function prints the given text to the standard output, and returns
/// the default result. In case of an error, an error message is printed to
/// the standard error and the returned result has
/// [`ExitStatus::FAILURE`](yash_env::semantics::ExitStatus::FAILURE). Any
/// errors that occur while printing the error message are ignored.
pub async fn output<S>(env: &mut Env<S>, content: &str) -> yash_env::builtin::Result
where
    S: Isatty + Fcntl + Write,
{
    match env.system.write_all(Fd::STDOUT, content.as_bytes()).await {
        Ok(_) => Default::default(),
        Err(errno) => {
            report::report_simple_failure(
                env,
                &format!("error printing results to stdout: {errno}"),
            )
            .await
        }
    }
}
