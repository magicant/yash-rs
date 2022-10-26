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

use crate::common::arg::parse_arguments;
use crate::common::arg::Mode;
use crate::common::print_error_message;
use crate::common::BuiltinEnv;
use std::future::Future;
use std::num::ParseIntError;
use std::ops::ControlFlow::Break;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;

async fn handle_error(env: &mut Env, title: &str, annotation: Annotation<'_>) -> Result {
    let message = Message {
        r#type: AnnotationType::Error,
        title: title.into(),
        annotations: vec![annotation],
    };
    print_error_message(env, message).await
}

async fn syntax_error(env: &mut Env, label: &str, location: &Location) -> Result {
    handle_error(
        env,
        "command argument syntax error",
        Annotation::new(AnnotationType::Error, label.into(), location),
    )
    .await
}

async fn operand_parse_error(env: &mut Env, location: &Location, error: ParseIntError) -> Result {
    syntax_error(env, &error.to_string(), location).await
}

async fn not_in_loop_error(env: &mut Env) -> Result {
    let builtin_name = &env.stack.builtin_name();
    let location = builtin_name.origin.clone();
    let title = if builtin_name.value == "continue" {
        "cannot continue"
    } else {
        "cannot break"
    };
    handle_error(
        env,
        title,
        Annotation::new(AnnotationType::Error, "not in loop".into(), &location),
    )
    .await
}

/// Implementation of the break built-in.
///
/// This function also implements the [continue](super::continue) built-in.
/// The behavior depends on the built-in name stored in the topmost stack
/// [frame](yash_env::stack::Frame) in the environment.
/// If the stack does not contain a `Frame::Builtin`, this function will
/// **panic!**
pub async fn builtin_body(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO: POSIX does not require the break built-in to support XBD Utility
    // Syntax Guidelines. That means the built-in does not have to recognize the
    // "--" separator. We should reject the separator in the POSIXly-correct mode.
    let (_options, operands) = match parse_arguments(&[], Mode::with_env(env), args) {
        Ok(result) => result,
        Err(error) => return print_error_message(env, &error).await,
    };
    if let Some(operand) = operands.get(1) {
        return syntax_error(env, "too many operands", &operand.origin).await;
    }

    let max_count = if let Some(operand) = operands.first() {
        match operand.value.parse() {
            Ok(0) => return syntax_error(env, "not a positive integer", &operand.origin).await,
            Ok(max_count) => max_count,
            Err(error) => return operand_parse_error(env, &operand.origin, error).await,
        }
    } else {
        1
    };

    let count = env.stack.loop_count(max_count);
    if count == 0 {
        not_in_loop_error(env).await
    } else {
        let count = count - 1;
        let divert = if env.stack.builtin_name().value == "continue" {
            Divert::Continue { count }
        } else {
            Divert::Break { count }
        };
        (ExitStatus::SUCCESS, Break(divert))
    }
}

/// Wrapper of [`builtin_body`] that returns the future in a pinned box.
pub fn builtin_main(env: &mut Env, args: Vec<Field>) -> Pin<Box<dyn Future<Output = Result> + '_>> {
    Box::pin(builtin_body(env, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use yash_env::stack::Frame;
    use yash_env::VirtualSystem;

    #[test]
    fn no_enclosing_loop() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("break"),
            is_special: true,
        });

        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::ERROR, Break(Divert::Interrupt(None))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("cannot break"), "{:?}", stderr);
            assert!(stderr.contains("not in loop"), "{:?}", stderr);
        });
    }

    #[test]
    fn omitted_operand_with_one_enclosing_loop() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("break"),
            is_special: true,
        });

        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(
            result,
            (ExitStatus::SUCCESS, Break(Divert::Break { count: 0 }))
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
            name: Field::dummy("break"),
            is_special: true,
        });

        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(
            result,
            (ExitStatus::SUCCESS, Break(Divert::Break { count: 0 }))
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
            name: Field::dummy("break"),
            is_special: true,
        });
        let args = Field::dummies(["1"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            (ExitStatus::SUCCESS, Break(Divert::Break { count: 0 }))
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
            name: Field::dummy("break"),
            is_special: true,
        });
        let args = Field::dummies(["3"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            (ExitStatus::SUCCESS, Break(Divert::Break { count: 2 }))
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
            name: Field::dummy("break"),
            is_special: true,
        });
        let args = Field::dummies(["5"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            (ExitStatus::SUCCESS, Break(Divert::Break { count: 3 }))
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
            name: Field::dummy("break"),
            is_special: true,
        });
        let args = Field::dummies(["0"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::ERROR, Break(Divert::Interrupt(None))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("not a positive integer"), "{:?}", stderr)
        });
    }

    #[test]
    fn explicit_operand_too_large() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("break"),
            is_special: true,
        });
        let args = Field::dummies(["999999999999999999999999999999"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::ERROR, Break(Divert::Interrupt(None))));
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
            name: Field::dummy("break"),
            is_special: true,
        });
        let args = Field::dummies(["1", "1"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::ERROR, Break(Divert::Interrupt(None))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("too many operands"), "{:?}", stderr)
        });
    }

    #[test]
    fn argument_separator() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Loop);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("break"),
            is_special: true,
        });
        let args = Field::dummies(["--", "1"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(
            result,
            (ExitStatus::SUCCESS, Break(Divert::Break { count: 0 }))
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }
}
