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

//! Break built-in
//!
//! The **`break`** built-in terminates the execution of a loop.
//!
//! # Syntax
//!
//! ```sh
//! break [n]
//! ```
//!
//! # Semantics
//!
//! `break n` quits the execution of the *n*th innermost `for`, `while`, or
//! `until` loop. The specified loop must lexically enclose the break command,
//! that is:
//!
//! - The loop is running in the same execution environment as the break
//!   command; and
//! - The break command appears inside the condition or body of the loop but not
//!   in the body of a function definition command appearing inside the loop.
//!
//! It is an error if there is no loop enclosing the break command.
//! If *n* is greater than the number of enclosing loops, the built-in exits the
//! outermost one.
//!
//! # Options
//!
//! None.
//!
//! (TODO: the `-i` option)
//!
//! # Operands
//!
//! Operand *n* specifies the nest level of the loop to exit.
//! If omitted, it defaults to 1.
//! It is an error if the value is not a positive decimal integer.
//!
//! # Exit status
//!
//! `ExitStatus::SUCCESS` or `ExitStatus::FAILURE` depending on the results
//!
//! # Portability
//!
//! The behavior is unspecified in POSIX when the break built-in is used without
//! an enclosing loop, in which case the current implementation returns an
//! error.
//!
//! POSIX allows the built-in to break a loop running in the current execution
//! environment that does not lexically enclose the break command. Our
//! implementation does not do that.
//!
//! # Implementation notes
//!
//! A successful invocation of the built-in returns a [`Result`] containing
//! `Break(Divert::Break(n-1))` as its `divert` field. The caller must pass the
//! value to enclosing loops so that the target loop can handle it.
//!
//! Part of the break built-in implementation is shared with the
//! continue built-in implementation.

use crate::common::report_error;
use crate::common::report_simple_error;
use yash_env::builtin::Result;
use yash_env::semantics::Field;
use yash_env::Env;

// pub mod display;
pub mod semantics;
pub mod syntax;

async fn report_semantics_error(env: &mut Env, error: &semantics::Error) -> Result {
    report_simple_error(env, &format!("cannot break: {}", error)).await
}

/// Entry point for executing the `break` built-in
///
/// This function uses the [`syntax`] and [`semantics`] modules to execute the built-in.
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    match syntax::parse(env, args) {
        Ok(count) => match semantics::run(&env.stack, count) {
            Ok(result) => result,
            Err(e) => report_semantics_error(env, &e).await,
        },
        Err(e) => report_error(env, &e).await,
    }
}
