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

//! Unset built-in
//!
//! The **`unset`** built-in unsets the values of shell variables.
//!
//! # Syntax
//!
//! ```sh
//! unset [-fv] name...
//! ```
//!
//! # Semantics
//!
//! The built-in unsets shell variables or functions named by the operands.
//!
//! # Options
//!
//! Either of the following options may be used to select what to unset:
//!
//! - The **`-v`** (**`--variables`**) option causes the built-in to unset shell variables.
//!   This is the default behavior.
//! - The **`-f`** (**`--functions`**) option causes the built-in to unset shell functions.
//!
//! (TODO: The `-l` (`--local`) option causes the built-in to unset local variables only.)
//!
//! # Operands
//!
//! Operands are the names of shell variables or functions to unset.
//!
//! # Exit status
//!
//! Zero unless an error occurs.
//!
//! # Errors
//!
//! Unsetting a read-only variable is an error.
//!
//! It is not an error to unset a variable or function that is not set.
//! The built-in ignores such operands.
//!
//! # Portability
//!
//! The behavior is not portable when both `-f` and `-v` are specified. Earlier
//! versions of yash used to honor the last specified option, but this version
//! errors out.
//!
//! If neither `-f` nor `-v` is specified and the variable named by an operand
//! is not set, POSIX allows the built-in to unset the same-named function if it
//! exists. Yash does not do this.
//!
//! (TODO TBD: In the POSIXly-correct mode, the built-in requires at least one operand.)
//!
//! When a global variable is hidden by a local variable, the current
//! implementation unsets the both. This is not portable. Old versions of yash
//! used to unset the local variable only.

use crate::common::print_error_message;
use crate::Result;
use yash_env::semantics::Field;
use yash_env::Env;

/// Selection of what to unset
#[derive(Debug, Clone, Copy, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Mode {
    /// Unsets shell variables.
    #[default]
    Variables,

    /// Unsets shell functions.
    Functions,
}

/// Parsed command line arguments
#[derive(Debug, Clone, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Command {
    /// What to unset
    pub mode: Mode,

    /// Names of shell variables or functions to unset
    pub names: Vec<Field>,
}

pub mod semantics;
pub mod syntax;

/// Entry point of the `unset` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    let command = match syntax::parse(env, args) {
        Ok(command) => command,
        Err(e) => return print_error_message(env, &e).await,
    };

    match command.mode {
        Mode::Variables => match semantics::unset_variables(env, &command.names) {
            Ok(()) => Result::default(),
            Err(errors) => semantics::report_variables_error(env, &errors).await,
        },

        Mode::Functions => match semantics::unset_functions(env, &command.names) {
            Ok(()) => Result::default(),
            Err(errors) => semantics::report_functions_error(env, &errors).await,
        },
    }
}
