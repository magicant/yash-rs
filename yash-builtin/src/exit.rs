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

//! Exit built-in
//!
//! The exit built-in causes the currently executing shell to exit.
//!
//! # Syntax
//!
//! ```sh
//! exit [exit_status]
//! ```
//!
//! # Options
//!
//! None. (TBD: non-portable extensions)
//!
//! # Operands
//!
//! The optional ***exit_status*** operand, if given, should be a non-negative
//! integer and will be the exit status of the exiting shell process.
//!
//! # Exit status
//!
//! The *exit_status* operand will be the exit status of the built-in.
//!
//! If the operand is not given:
//!
//! - If the currently executing script is a trap, the exit status will be the
//!   value of `$?` before entering the trap.
//! - Otherwise, the exit status will be the current value of `$?`.
//!
//! # Errors
//!
//! If the *exit_status* operand is given but not a valid non-negative integer,
//! it is a syntax error. In that case, an error message is printed, and the
//! exit status will be 2 ([`ExitStatus::ERROR`]). The shell will still exit.
//!
//! This implementation treats an *exit_status* value greater than 4294967295 as
//! a syntax error.
//!
//! # Portability
//!
//! Many implementations do not support *exit_status* values greater than 255.
//!
//! # Implementation notes
//!
//! This implementation of the built-in does not actually exit the shell, but
//! returns a [`Result`] having a [`Divert::Exit`]. The caller is responsible
//! for handling the divert value and exiting the process.

use std::future::ready;
use std::future::Future;
use std::pin::Pin;
use yash_env::builtin::Result;
#[cfg(doc)]
use yash_env::semantics::Divert;
#[cfg(doc)]
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::Env;

/// Implementation of the exit built-in.
///
/// See the [module-level documentation](self) for details.
pub fn builtin_main_sync(_env: &mut Env, _args: Vec<Field>) -> Result {
    // TODO
    Result::default()
}

/// Implementation of the exit built-in.
///
/// This function calls [`builtin_main_sync`] and wraps the result in a
/// `Future`.
pub fn builtin_main(
    env: &mut yash_env::Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result>>> {
    Box::pin(ready(builtin_main_sync(env, args)))
}
