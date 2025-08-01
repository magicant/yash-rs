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
//! This module implements the [`return` built-in], which quits the currently
//! executing innermost function or script.
//!
//! [`return` built-in]: https://magicant.github.io/yash-rs/builtins/return.html
//!
//! # Implementation notes
//!
//! This implementation of the built-in does not actually quit the current
//! function or dot script, but returns a [`Result`] having a
//! [`Divert::Return`]. The caller is responsible for handling the divert value
//! and returning from the function or script.
//!
//! - If an operand specifies an exit status, the divert value will contain the
//!   specified exit status. The caller should use it as the exit status of the
//!   process.
//! - If no operand is given, the divert value will contain no exit status. The
//!   built-in's exit status is the current value of `$?`, and the caller should
//!   use it as the exit status of the function or script. However, if the
//!   built-in is invoked in a trap executed in the function or script, the
//!   caller should use the value of `$?` before entering trap.

use crate::common::syntax_error;
use std::num::ParseIntError;
use std::ops::ControlFlow::Break;
use yash_env::Env;
use yash_env::builtin::Result;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_syntax::source::Location;

// TODO Split into syntax and semantics submodules

async fn operand_parse_error(env: &mut Env, location: &Location, error: ParseIntError) -> Result {
    syntax_error(env, &error.to_string(), location).await
}

/// Entry point for executing the `return` built-in
///
/// See the [module-level documentation](self) for details.
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO: POSIX does not require the return built-in to support XBD Utility
    // Syntax Guidelines. That means the built-in does not have to recognize the
    // "--" separator. We should reject the separator in the POSIXly-correct
    // mode.
    // TODO Reject returning from an interactive session

    let mut i = args.iter().peekable();

    let no_return = i.next_if(|field| field.value == "-n").is_some();

    let exit_status = match i.next() {
        None => None,
        Some(arg) => match arg.value.parse() {
            Ok(exit_status) if exit_status >= 0 => Some(ExitStatus(exit_status)),
            Ok(_) => return syntax_error(env, "negative exit status", &arg.origin).await,
            Err(e) => return operand_parse_error(env, &arg.origin, e).await,
        },
    };

    // `i` is fused, so it's safe to call next() again
    if let Some(arg) = i.next() {
        return syntax_error(env, "too many operands", &arg.origin).await;
    }

    if no_return {
        Result::new(exit_status.unwrap_or(env.exit_status))
    } else {
        Result::with_exit_status_and_divert(env.exit_status, Break(Divert::Return(exit_status)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::semantics::ExitStatus;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env_test_helper::assert_stderr;

    #[test]
    fn return_without_arguments_with_exit_status_0() {
        let mut env = Env::new_virtual();
        let actual_result = main(&mut env, vec![]).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::SUCCESS, Break(Divert::Return(None)));
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn return_without_arguments_with_non_zero_exit_status() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus(42);
        let actual_result = main(&mut env, vec![]).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus(42), Break(Divert::Return(None)));
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn returns_exit_status_specified_without_n_option() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["42"]);
        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result = Result::with_exit_status_and_divert(
            ExitStatus::SUCCESS,
            Break(Divert::Return(Some(ExitStatus(42)))),
        );
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn returns_exit_status_12_with_n_option() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["-n", "12"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus(12)));
    }

    #[test]
    fn returns_exit_status_47_with_n_option() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus(24);
        let args = Field::dummies(["-n", "47"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus(47)));
    }

    #[test]
    fn returns_previous_exit_status_with_n_option_without_operand() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus(24);
        let args = Field::dummies(["-n"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus(24)));
    }

    #[test]
    fn return_with_negative_exit_status_operand() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("return"),
            is_special: true,
        }));
        let args = Field::dummies(["-1"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::ERROR, Break(Divert::Interrupt(None)));
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
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("return"),
            is_special: true,
        }));
        let args = Field::dummies(["foo"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::ERROR, Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("foo"), "stderr = {stderr:?}")
        });
    }

    #[test]
    fn return_with_too_large_exit_status_operand() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("return"),
            is_special: true,
        }));
        let args = Field::dummies(["999999999999999999999999999999"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::ERROR, Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(
                stderr.contains("999999999999999999999999999999"),
                "stderr = {stderr:?}"
            )
        });
    }

    #[test]
    fn return_with_too_many_operands() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("return"),
            is_special: true,
        }));
        let args = Field::dummies(["1", "2"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::ERROR, Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("too many operands"), "stderr = {stderr:?}")
        });
    }

    // TODO return_with_invalid_option
    // TODO return used outside a function or script
}
