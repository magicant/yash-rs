// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! False built-in
//!
//! The **`false`** built-in command does nothing, unsuccessfully.
//!
//! # Synopsis
//!
//! ```sh
//! false
//! ```
//!
//! # Description
//!
//! The `false` built-in command does nothing and returns a non-zero exit
//! status.
//!
//! # Options
//!
//! None.
//!
//! # Operands
//!
//! None.
//!
//! # Errors
//!
//! None.
//!
//! (TODO: In the future, the built-in may detect unexpected options or operands.)
//!
//! # Exit Status
//!
//! [`ExitStatus::FAILURE`].
//!
//! # Portability
//!
//! POSIX allows the `false` built-in to return any non-zero exit status, but
//! most implementations return one.
//!
//! Most implementations ignore any arguments, but some implementations may
//! accept them. For example, the GNU coreutils implementation accepts the
//! `--help` and `--version` options. For maximum portability, avoid passing
//! arguments to the `false` command.

use crate::Result;
use yash_env::Env;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;

/// Executes the `false` built-in.
///
/// This is the main entry point for the `false` built-in.
pub async fn main(_env: &mut Env, _args: Vec<Field>) -> Result {
    Result::new(ExitStatus::FAILURE)
}
