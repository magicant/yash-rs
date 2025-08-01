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
//! This module implements the [`source` (`.`) built-in], which reads and executes
//! commands from a file.
//!
//! [`source` (`.`) built-in]: https://magicant.github.io/yash-rs/builtins/source.html

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
