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

//! Source (`.`) built-in
//!
//! The **`source`** (**`.`**) built-in reads and executes commands from a file.
//!
//! # Synopsis
//!
//! ```sh
//! . file [arguments…]
//! ```
//!
//! ```sh
//! source file [arguments…]
//! ```
//!
//! # Description
//!
//! The `.` built-in reads and executes commands from the specified file in the
//! current shell environment.
//!
//! If the filename does not contain a slash, the shell searches the directories
//! in the `$PATH` variable for the file. The file does not need to be
//! executable; any readable file found first is used. (TODO: If no file is
//! found, the built-in falls back to the file in the current working
//! directory.)
//!
//! If there are any operands after the filename, they are assigned to the
//! positional parameters (`$1`, `$2`, etc.) during the execution of the file.
//! In this case, a regular [variable context](yash_env::variable::Context) is
//! pushed to secure the positional parameters. The context will also affect
//! local variables possibly defined in the file. The context is popped when the
//! execution of the file is finished. No context is pushed if there are no
//! operands other than the filename.
//!
//! # Options
//!
//! None.
//!
//! (TODO: non-portable options)
//!
//! # Operands
//!
//! The first operand ***file*** must be given and is the pathname of the file
//! to be executed. If it does not contain a slash, it is subject to the search
//! described above.
//!
//! Any remaining ***arguments*** are passed to the executed file as positional
//! parameters.
//!
//! # Errors
//!
//! It is an error if the file cannot be found or read.
//! During parsing and execution, any syntax error or runtime error may occur.
//!
//! # Exit status
//!
//! The exit status of the source built-in is the exit status of the last
//! command executed in the file.
//! If there is no command in the file, the exit status is zero.
//!
//! If the file cannot be found or read, the exit status is 1
//! ([`ExitStatus::FAILURE`]).
//! In case of a syntax error in the file, the exit status is 2
//! ([`ExitStatus::ERROR`]).
//!
//! # Portability
//!
//! The `.` built-in is specified in the POSIX standard. The built-in name
//! `source` is a non-portable extension that is also available in some other
//! shells.
//!
//! POSIX does not require the `.` built-in to conform to the Utility Syntax
//! Guidelines, which means portable scripts cannot use any options or the `--`
//! separator for the built-in.
//!
//! Falling back to the file in the current working directory when the file is
//! not found in the `$PATH` is a non-portable extension. This behavior is
//! disabled if the TBD shell option is set. The result of the `$PATH` search
//! may be unpredictable depending on the environment. Prefix the filename with
//! `./` to avoid the search and make sure the file in the current working
//! directory is used.
//!
//! Setting the positional parameters with additional operands is a non-portable
//! extension that is supported by some other shells. The behavior about the
//! local variable context may differ in other shells.
//!
//! Other implementations may return a different non-zero exit status for an
//! error.

use crate::Result;
use yash_env::Env;
#[cfg(doc)]
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;

mod semantics;
pub mod syntax;

/// Set of information that defines the behavior of a single invocation of the
/// `.` built-in
///
/// The [`syntax::parse`] function parses the command line arguments and
/// constructs a `Command` value. The [`execute`](Self::execute) method
/// executes the command.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Command {
    /// Pathname of the file to be executed
    pub file: Field,
    /// Arguments to be passed to the file
    pub params: Vec<Field>,
}

/// Entry point of the `.` built-in execution
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    match syntax::parse(env, args) {
        Ok(command) => command.execute(env).await,
        Err(error) => error.report(env).await,
    }
}
