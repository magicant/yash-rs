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

//! Return built-in.
//!
//! The return built-in quits the currently executing innermost function or
//! script.  If the shell is not currently executing any function or script, the
//! built-in will exit the current shell execution environment unless it is an
//! interactive session.
//!
//! # Syntax
//!
//! ```sh
//! return [-n] [exit_status]
//! ```
//!
//! # Options
//!
//! The **`-n`** (**`--no-return`**) option makes the built-in not actually quit
//! a function or script. This option will be helpful when you want to set the
//! exit status to an arbitrary value without any other side effect.
//!
//! # Operands
//!
//! The optional ***exit_status*** operand, if given, should be a non-negative
//! integer and will be the exit status of the built-in.
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
//! exit status will be 2, but the built-in still quits a function or script.
//!
//! This implementation treats an *exit_status* value greater than 4294967295 as
//! a syntax error.
//!
//! # Portability
//!
//! POSIX only requires the return built-in to quit a function or dot script.
//! The behavior for other kinds of scripts is a non-standard extension.
//!
//! The `-n` (`--no-return`) option is a non-standard extension.
//!
//! Many implementations do not support *exit_status* values greater than 255.
//!
//! # Implementation notes
//!
//! TODO Describe in terms of Divert.

use std::future::ready;
use std::future::Future;
use std::ops::ControlFlow::{Break, Continue};
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::exec::Divert;
use yash_env::exec::ExitStatus;
use yash_env::expansion::Field;

/// Part of the shell execution environment the return built-in depends on.
pub trait Env {
    // TODO Current value of $?
    // TODO Current execution context (stack trace)
    // TODO stderr
}

impl Env for yash_env::Env {}

/// Implementation of the return built-in.
///
/// See the [module-level documentation](self) for details.
pub fn builtin_main_sync<E: Env>(_env: &mut E, args: Vec<Field>) -> Result {
    // TODO Parse arguments correctly
    // TODO Reject returning from an interactive session
    let mut i = args.iter().skip(1).peekable();
    let no_return = matches!(i.peek(), Some(Field { value, .. }) if value == "-n");
    if no_return {
        i.next();
    }
    let exit_status = match i.next() {
        Some(field) => field.value.parse().unwrap_or(2),
        None => 0,
    };
    let flow = if no_return {
        Continue(())
    } else {
        Break(Divert::Return)
    };
    (ExitStatus(exit_status), flow)
}

/// Implementation of the return built-in.
///
/// This function calls [`builtin_main_sync`] and wraps the result in a
/// `Future`.
pub fn builtin_main(
    env: &mut yash_env::Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result>>> {
    Box::pin(ready(builtin_main_sync(env, args)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::exec::ExitStatus;

    #[derive(Default)]
    struct DummyEnv;

    impl Env for DummyEnv {}

    #[test]
    fn returns_exit_status_specified_without_n_option() {
        let mut env = DummyEnv::default();
        let args = Field::dummies(["return", "42"]);
        let result = builtin_main_sync(&mut env, args);
        assert_eq!(result, (ExitStatus(42), Break(Divert::Return)));
    }

    #[test]
    fn returns_exit_status_12_with_n_option() {
        let mut env = DummyEnv::default();
        let args = Field::dummies(["return", "-n", "12"]);
        let result = builtin_main_sync(&mut env, args);
        assert_eq!(result, (ExitStatus(12), Continue(())));
    }

    #[test]
    fn returns_exit_status_47_with_n_option() {
        let mut env = DummyEnv::default();
        let args = Field::dummies(["return", "-n", "47"]);
        let result = builtin_main_sync(&mut env, args);
        assert_eq!(result, (ExitStatus(47), Continue(())));
    }
}
