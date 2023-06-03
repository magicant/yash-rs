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
//! The **`exit`** built-in causes the currently executing shell to exit.
//!
//! # Syntax
//!
//! ```sh
//! exit [exit_status]
//! ```
//!
//! # Semantics
//!
//! `exit exit_status` makes the shell exit from the currently executing
//! environment with the specified exit status.
//!
//! The shell executes the EXIT trap, if any, before exiting, except when the
//! built-in is invoked in the trap itself.
//!
//! # Options
//!
//! None. (TBD: non-portable extensions)
//!
//! # Operands
//!
//! The optional ***exit_status*** operand, if given, should be a non-negative
//! decimal integer and will be the exit status of the exiting shell process.
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
//! exit status will be 2 ([`ExitStatus::ERROR`]).
//!
//! This implementation treats an *exit_status* value greater than 2147483647 as
//! a syntax error.
//!
//! # Portability
//!
//! The behavior is undefined in POSIX if *exit_status* is greater than 255.
//! The current implementation passes such a value as is in the result, but this
//! behavior may change in the future.
//!
//! # Implementation notes
//!
//! This implementation of the built-in does not actually exit the shell, but
//! returns a [`Result`] having a [`Divert::Exit`]. The caller is responsible
//! for handling the divert value and exiting the process.
//!
//! In case of an error, the result will have a [`Divert::Interrupt`] value
//! instead, in which case the shell will not exit if it is interactive.

use crate::common::syntax_error;
use std::future::Future;
use std::num::ParseIntError;
use std::ops::ControlFlow::Break;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_syntax::source::Location;

async fn operand_parse_error(env: &mut Env, location: &Location, error: ParseIntError) -> Result {
    syntax_error(env, &error.to_string(), location).await
}

/// Implementation of the exit built-in.
///
/// See the [module-level documentation](self) for details.
pub async fn builtin_body(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO: POSIX does not require the break built-in to support XBD Utility
    // Syntax Guidelines. That means the built-in does not have to recognize the
    // "--" separator. We should reject the separator in the POSIXly-correct mode.

    if let Some(arg) = args.get(1) {
        return syntax_error(env, "too many operands", &arg.origin).await;
    }

    let exit_status = match args.first() {
        None => env.stack.previous_exit_status().unwrap_or(env.exit_status),
        Some(arg) => match arg.value.parse() {
            Ok(exit_status) if exit_status >= 0 => ExitStatus(exit_status),
            Ok(_) => return syntax_error(env, "negative exit status", &arg.origin).await,
            Err(e) => return operand_parse_error(env, &arg.origin, e).await,
        },
    };
    let mut result = Result::new(exit_status);
    result.set_divert(Break(Divert::Exit(None)));
    result
}

/// Implementation of the exit built-in.
///
/// This function calls [`builtin_body`] and wraps the result in a
/// `Future`.
pub fn builtin_main(
    env: &mut yash_env::Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result> + '_>> {
    Box::pin(builtin_body(env, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use yash_env::stack::Frame;
    use yash_env::VirtualSystem;

    #[test]
    fn exit_without_arguments_with_exit_status_0() {
        let mut env = Env::new_virtual();
        let actual_result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        let mut expected_result = Result::default();
        expected_result.set_divert(Break(Divert::Exit(None)));
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn exit_without_arguments_with_non_zero_exit_status() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus(42);
        let actual_result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        let mut expected_result = Result::new(ExitStatus(42));
        expected_result.set_divert(Break(Divert::Exit(None)));
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn exit_without_arguments_inside_trap() {
        let mut env = Env::new_virtual();
        let mut env = env.push_frame(Frame::Trap {
            condition: yash_env::trap::Condition::Signal(yash_env::trap::Signal::SIGINT),
            previous_exit_status: ExitStatus(123),
        });
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("exit"),
            is_special: true,
        });
        env.exit_status = ExitStatus(42);

        let actual_result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        let mut expected_result = Result::new(ExitStatus(123));
        expected_result.set_divert(Break(Divert::Exit(None)));
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn exit_with_exit_status_operand() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["123"]);
        let actual_result = builtin_body(&mut env, args).now_or_never().unwrap();
        let mut expected_result = Result::new(ExitStatus(123));
        expected_result.set_divert(Break(Divert::Exit(None)));
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn exit_with_negative_exit_status_operand() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("exit"),
            is_special: true,
        });
        let args = Field::dummies(["-1"]);

        let actual_result = builtin_body(&mut env, args).now_or_never().unwrap();
        let mut expected_result = Result::new(ExitStatus::ERROR);
        expected_result.set_divert(Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("-1"), "stderr = {stderr:?}")
        });
    }

    #[test]
    fn exit_with_non_integer_exit_status_operand() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("exit"),
            is_special: true,
        });
        let args = Field::dummies(["foo"]);

        let actual_result = builtin_body(&mut env, args).now_or_never().unwrap();
        let mut expected_result = Result::new(ExitStatus::ERROR);
        expected_result.set_divert(Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("foo"), "stderr = {stderr:?}")
        });
    }

    #[test]
    fn exit_with_too_large_exit_status_operand() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("exit"),
            is_special: true,
        });
        let args = Field::dummies(["999999999999999999999999999999"]);

        let actual_result = builtin_body(&mut env, args).now_or_never().unwrap();
        let mut expected_result = Result::new(ExitStatus::ERROR);
        expected_result.set_divert(Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(
                stderr.contains("999999999999999999999999999999"),
                "stderr = {stderr:?}"
            )
        });
    }

    #[test]
    fn exit_with_too_many_arguments() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("exit"),
            is_special: true,
        });
        let args = Field::dummies(["1", "2"]);

        let actual_result = builtin_body(&mut env, args).now_or_never().unwrap();
        let mut expected_result = Result::new(ExitStatus::ERROR);
        expected_result.set_divert(Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("too many operands"), "stderr = {stderr:?}")
        });
    }

    // TODO exit_with_invalid_option
    // TODO exit_from_interactive_shell_without_suspended_job
    // TODO exit_from_interactive_shell_with_suspended_job_in_posix_mode
    // TODO exit_from_interactive_shell_with_suspended_job_not_in_posix_mode
    // TODO force_exit_from_interactive_shell_with_suspended_job
}
