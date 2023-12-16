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

//! Unalias built-in
//!
//! The **`unalias`** built-in removes alias definitions.
//!
//! # Synopsis
//!
//! ```sh
//! unalias name…
//! ```
//!
//! ```sh
//! unalias -a
//! ```
//!
//! # Description
//!
//! The unalias built-in removes alias definitions as specified by the operands.
//!
//! # Options
//!
//! The **`-a`** (**`--all`**) option removes all alias definitions.
//!
//! # Operands
//!
//! Each operand must be the name of an alias to remove.
//!
//! # Errors
//!
//! It is an error if an operand names a non-existent alias.
//!
//! # Exit status
//!
//! Zero unless an error occurs.
//!
//! # Portability
//!
//! The unalias built-in is specified in POSIX.
//!
//! Some shells implement some built-in utilities as predefined aliases. Using
//! `unalias -a` may make such built-ins unavailable.

use crate::common::report_error;
use yash_env::semantics::Field;
use yash_env::Env;

/// Parsed command arguments for the `unalias` built-in
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    /// Remove specified aliases
    Remove(Vec<Field>),
    /// Remove all aliases
    RemoveAll,
}

pub mod semantics;
pub mod syntax;

/// Entry point for executing the `unalias` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    match syntax::parse(env, args) {
        Ok(command) => {
            let errors = command.execute(env);

            let mut result = crate::Result::default();
            for error in errors {
                result = result.max(report_error(env, &error).await);
            }
            result
        }

        Err(e) => report_error(env, e.to_message()).await,
    }
}
