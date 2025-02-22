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

//! True built-in
//!
//! The **`true`** built-in command does nothing, successfully.
//!
//! # Synopsis
//!
//! ```sh
//! true
//! ```
//!
//! # Description
//!
//! The `true` built-in command does nothing, successfully. It is useful as a
//! placeholder when a command is required but no action is needed.
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
//! Zero.
//!
//! # Portability
//!
//! Most implementations ignore any arguments, but some implementations may
//! accept them. For example, the GNU coreutils implementation accepts the
//! `--help` and `--version` options. For maximum portability, avoid passing
//! arguments to the `true` command.

use crate::Result;
use yash_env::Env;
use yash_env::semantics::Field;

/// Executes the `true` built-in.
///
/// This is the main entry point for the `true` built-in.
pub async fn main(_env: &mut Env, _args: Vec<Field>) -> Result {
    Result::default()
}
