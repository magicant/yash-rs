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
//! The `break` built-in terminates the execution of a loop.
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
//! If `n` is greater than the number of enclosing loops, the built-in exits the
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
//! Operand `n` specifies the nest level of the loop to exit.
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
//! A successful invocation of the built-in returns `Break(Divert::Break(n-1))`
//! as the second element of the returned tuple. The caller must pass the value
//! to enclosing loops so that the target loop can handle it.

use std::future::Future;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::semantics::Field;
use yash_env::Env;

/// Implementation of the break built-in.
pub async fn builtin_body(_env: &mut Env, _args: Vec<Field>) -> Result {
    todo!()
}

/// Wrapper of [`builtin_body`] that returns the future in a pinned box.
pub fn builtin_main(env: &mut Env, args: Vec<Field>) -> Pin<Box<dyn Future<Output = Result> + '_>> {
    Box::pin(builtin_body(env, args))
}

#[cfg(test)]
mod tests {
    // use super::*;

    // TODO no_enclosing_loop
    // TODO omitted_operand_with_one_enclosing_loop
    // TODO omitted_operand_with_many_enclosing_loops
    // TODO explicit_operand_1_with_many_enclosing_loops
    // TODO explicit_operand_3_with_many_enclosing_loops
    // TODO explicit_operand_greater_than_number_of_loops
    // TODO malformed_operands
}
