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

//! Continue built-in
//!
//! This module implements the [`continue` built-in], which skips the execution
//! of a loop to the next iteration.
//!
//! [`continue` built-in]: https://magicant.github.io/yash-rs/builtins/continue.html
//!
//! # Implementation notes
//!
//! A successful invocation of the built-in returns a [`Result`] containing
//! `Break(Divert::Continue(n-1))` as its `divert` field. The caller must pass
//! the value to enclosing loops so that the target loop can handle it.
//!
//! Part of the continue built-in implementation is shared with the
//! break built-in implementation.
//! This module re-exports [`super::break::syntax`].

use crate::common::report_error;
use crate::common::report_simple_failure;
use yash_env::Env;
use yash_env::builtin::Result;
use yash_env::semantics::Field;

// pub mod display;
pub mod semantics;
pub use super::r#break::syntax;

/// Entry point for executing the `continue` built-in
///
/// This function uses the [`syntax`] and [`semantics`] modules to execute the built-in.
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    match syntax::parse(env, args) {
        Ok(count) => match semantics::run(&env.stack, count) {
            Ok(result) => result,
            Err(e) => report_simple_failure(env, &format!("cannot continue: {e}")).await,
        },
        Err(e) => report_error(env, &e).await,
    }
}

// TODO Replace with scripted integration tests
#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use yash_env::Env;
    use yash_env::VirtualSystem;
    use yash_env::builtin::Result;
    use yash_env::semantics::Divert;
    use yash_env::semantics::ExitStatus;
    use yash_env::semantics::Field;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;

    fn result_with_divert(exit_status: ExitStatus, divert: Divert) -> Result {
        let mut result = Result::new(exit_status);
        result.set_divert(Break(divert));
        result
    }

    #[test]
    fn no_enclosing_loop() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        }));

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::FAILURE, Divert::Interrupt(None))
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("cannot continue"), "stderr = {stderr:?}");
            assert!(stderr.contains("not in a loop"), "stderr = {stderr:?}");
        });
    }

    #[test]
    fn omitted_operand_with_one_enclosing_loop() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        }));

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::SUCCESS, Divert::Continue { count: 0 })
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn omitted_operand_with_many_enclosing_loops() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        }));

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::SUCCESS, Divert::Continue { count: 0 })
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn explicit_operand_1_with_many_enclosing_loops() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        }));
        let args = Field::dummies(["1"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::SUCCESS, Divert::Continue { count: 0 })
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn explicit_operand_3_with_many_enclosing_loops() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        }));
        let args = Field::dummies(["3"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::SUCCESS, Divert::Continue { count: 2 })
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn explicit_operand_greater_than_number_of_loops() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        }));
        let args = Field::dummies(["5"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::SUCCESS, Divert::Continue { count: 3 })
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn explicit_operand_0() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        }));
        let args = Field::dummies(["0"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::ERROR, Divert::Interrupt(None))
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(
                stderr.contains("invalid numeric operand"),
                "stderr = {stderr:?}"
            )
        });
    }

    #[test]
    fn explicit_operand_too_large() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        }));
        let args = Field::dummies(["999999999999999999999999999999"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::ERROR, Divert::Interrupt(None))
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn too_many_operands() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        }));
        let args = Field::dummies(["1", "1"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::ERROR, Divert::Interrupt(None))
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("too many operands"), "stderr = {stderr:?}")
        });
    }

    #[test]
    fn argument_separator() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        }));
        let args = Field::dummies(["--", "1"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::SUCCESS, Divert::Continue { count: 0 })
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }
}
