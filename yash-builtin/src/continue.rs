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
//! The **`continue`** built-in skips the execution of a loop to the next
//! iteration.
//!
//! # Syntax
//!
//! ```sh
//! continue [n]
//! ```
//!
//! # Semantics
//!
//! `continue n` interrupts the execution of the *n*th innermost for, while, or
//! until loop and resumes its next iteration.
//! The specified loop must lexically enclose the continue command, that is:
//!
//! - The loop is running in the same execution environment as the continue
//!   command; and
//! - The continue command appears inside the condition or body of the loop but
//!   not in the body of a function definition command appearing inside the
//!   loop.
//!
//! It is an error if there is no loop enclosing the continue command.
//! If *n* is greater than the number of enclosing loops, the built-in affects
//! the outermost one.
//!
//! # Options
//!
//! None.
//!
//! (TODO: the -i option)
//!
//! # Operands
//!
//! Operand *n* specifies the nest level of the affected loop.
//! If omitted, it defaults to 1. It is an error if the value is not a positive
//! decimal integer.
//!
//! # Exit status
//!
//! `ExitStatus::SUCCESS` or `ExitStatus::FAILURE` depending on the results
//!
//! # Portability
//!
//! The behavior is unspecified in POSIX when the continue built-in is used
//! without an enclosing loop, in which case the current implementation returns
//! an error.
//!
//! POSIX allows the built-in to restart a loop running in the current execution
//! environment that does not lexically enclose the continue command.
//! Our implementation declines to do that.
//!
//! # Implementation notes
//!
//! A successful invocation of the built-in returns
//! `Break(Divert::Continue(n-1))` as the second element of the returned tuple.
//! The caller must pass the value to enclosing loops so that the target loop
//! can handle it.
//!
//! The implementation of the continue built-in is shared with the
//! break built-in.
//! This module just re-exports functions from [`super::break`].

pub use super::r#break::{builtin_body, builtin_main};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use futures_util::FutureExt;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use yash_env::builtin::Result;
    use yash_env::semantics::Divert;
    use yash_env::semantics::ExitStatus;
    use yash_env::semantics::Field;
    use yash_env::stack::Frame;
    use yash_env::Env;
    use yash_env::VirtualSystem;

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
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        });

        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::ERROR, Divert::Interrupt(None))
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("cannot continue"), "stderr = {stderr:?}");
            assert!(stderr.contains("not in loop"), "stderr = {stderr:?}");
        });
    }

    #[test]
    fn omitted_operand_with_one_enclosing_loop() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        });

        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
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
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        });

        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
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
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        });
        let args = Field::dummies(["1"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
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
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        });
        let args = Field::dummies(["3"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
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
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        });
        let args = Field::dummies(["5"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
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
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        });
        let args = Field::dummies(["0"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::ERROR, Divert::Interrupt(None))
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(
                stderr.contains("not a positive integer"),
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
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        });
        let args = Field::dummies(["999999999999999999999999999999"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
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
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        });
        let args = Field::dummies(["1", "1"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
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
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        });
        let args = Field::dummies(["--", "1"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            result_with_divert(ExitStatus::SUCCESS, Divert::Continue { count: 0 })
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }
}
