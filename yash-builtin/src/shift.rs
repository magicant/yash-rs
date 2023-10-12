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

//! Shift built-in
//!
//! The **`shift`** built-in removes some positional parameters.
//!
//! # Syntax
//!
//! ```sh
//! shift [n]
//! ```
//!
//! # Semantics
//!
//! The built-in removes the first `n` positional parameters from the list of
//! positional parameters. If `n` is omitted, it is assumed to be `1`.
//!
//! # Options
//!
//! None. (TBD: non-portable extensions)
//!
//! # Operands
//!
//! The operand specifies the number of positional parameters to remove. It must
//! be a non-negative decimal integer less than or equal to the number of
//! positional parameters.
//!
//! # Exit status
//!
//! Zero unless an error occurs.
//!
//! # Errors
//!
//! It is an error to try to remove more than the number of existing positional
//! parameters.
//!
//! # Portability
//!
//! POSIX does not specify whether an invalid operand is a syntax error or a
//! runtime error. This implementation treats it as a syntax error.
//!
//! (TODO: the array option and negative operands)
//!
//! # Implementation notes
//!
//! This built-in expects the [positional
//! parameters](yash_env::variable::VariableSet::positional_params_mut) to be an
//! array. If it is not an array, the built-in panics.

use crate::common::arrange_message_and_divert;
use crate::common::syntax_error;
use std::borrow::Cow;
use yash_env::builtin::Result;
use yash_env::io::Stderr;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::variable::Value;
use yash_env::Env;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;

pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO: POSIX does not require the shift built-in to support XBD Utility
    // Syntax Guidelines. That means the built-in does not have to recognize the
    // "--" separator. We should reject the separator in the POSIXly-correct
    // mode.

    if let Some(arg) = args.get(1) {
        return syntax_error(env, "too many operands", &arg.origin).await;
    }

    let (count, operand_location) = match args.first() {
        None => (1, None),
        Some(arg) => {
            let count = match arg.value.parse() {
                Ok(count) => count,
                Err(e) => {
                    let message = format!("non-integral operand: {e}");
                    return syntax_error(env, &message, &arg.origin).await;
                }
            };
            (count, Some(&arg.origin))
        }
    };

    let params = env.variables.positional_params_mut();
    let values = match params.value.as_mut() {
        None => panic!("positional parameters are undefined"),
        Some(Value::Scalar(value)) => panic!("positional parameters are not an array: {value:?}"),
        Some(Value::Array(params)) => params,
    };

    if values.len() < count {
        // Failure: cannot shift so many positional parameters
        let (label, location) = match operand_location {
            None => (
                "there are no positional parameters".into(),
                env.stack.current_builtin().map_or_else(
                    || Cow::Owned(Location::dummy("")),
                    |b| Cow::Borrowed(&b.name.origin),
                ),
            ),
            Some(location) => (
                format!(
                    "requested to shift {} but there {} only {}",
                    count,
                    if values.len() == 1 { "is" } else { "are" },
                    values.len(),
                )
                .into(),
                Cow::Borrowed(location),
            ),
        };
        let last_location = params.last_assigned_location.clone();
        let mut annotations = vec![Annotation::new(AnnotationType::Error, label, &location)];
        if let Some(last_location) = &last_location {
            annotations.push(Annotation::new(
                AnnotationType::Info,
                "positional parameters were last modified here".into(),
                last_location,
            ));
        }
        let message = Message {
            r#type: AnnotationType::Error,
            title: "cannot shift positional parameters".into(),
            annotations,
        };
        let (message, divert) = arrange_message_and_divert(env, message);
        env.system.print_error(&message).await;
        return Result::with_exit_status_and_divert(ExitStatus::FAILURE, divert);
    }

    values.drain(..count);
    params.last_assigned_location = env.stack.current_builtin().map(|b| b.name.origin.clone());
    Result::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use futures_util::FutureExt;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use std::vec;
    use yash_env::semantics::Divert;
    use yash_env::semantics::ExitStatus;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env::VirtualSystem;
    use yash_syntax::source::Location;

    #[test]
    fn shifting_without_operand() {
        let mut env = Env::new_virtual();
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("shift"),
            is_special: true,
        }));
        env.variables.positional_params_mut().value = Some(Value::array(["1", "2", "3"]));

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        assert_eq!(
            env.variables.positional_params().value,
            Some(Value::array(["2", "3"])),
        );

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        assert_eq!(
            env.variables.positional_params().value,
            Some(Value::array(["3"])),
        );

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        let params = env.variables.positional_params();
        assert_eq!(params.value, Some(Value::Array(vec![])));
        assert_eq!(
            params.last_assigned_location,
            Some(Location::dummy("shift")),
        );
    }

    #[test]
    fn shifting_with_operand() {
        let mut env = Env::new_virtual();
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("shift"),
            is_special: true,
        }));
        env.variables.positional_params_mut().value =
            Some(Value::array(["1", "2", "3", "4", "5", "6", "7"]));

        let args = Field::dummies(["2"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        assert_eq!(
            env.variables.positional_params().value,
            Some(Value::array(["3", "4", "5", "6", "7"])),
        );

        let args = Field::dummies(["3"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        assert_eq!(
            env.variables.positional_params().value,
            Some(Value::array(["6", "7"])),
        );

        let args = Field::dummies(["2"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        assert_eq!(
            env.variables.positional_params().value,
            Some(Value::Array(vec![])),
        );
    }

    #[test]
    fn shifting_without_operand_without_params() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("shift"),
            is_special: true,
        }));

        let actual_result = main(&mut env, vec![]).now_or_never().unwrap();
        let expected_result = Result::with_exit_status_and_divert(
            ExitStatus::FAILURE,
            Break(Divert::Interrupt(None)),
        );
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(
                stderr.contains("there are no positional parameters"),
                "stderr = {stderr:?}",
            )
        });
    }

    #[test]
    fn shifting_more_than_the_number_of_params() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("shift"),
            is_special: true,
        }));
        env.variables.positional_params_mut().value = Some(Value::array(["1", "2", "3"]));

        let args = Field::dummies(["4"]);
        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result = Result::with_exit_status_and_divert(
            ExitStatus::FAILURE,
            Break(Divert::Interrupt(None)),
        );
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(
                stderr.contains("requested to shift 4 but there are only 3"),
                "stderr = {stderr:?}",
            )
        });
    }

    #[test]
    fn non_integral_operand_in_posix_mode() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        // TODO Enable POSIX mode
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("shift"),
            is_special: true,
        }));
        let args = Field::dummies(["1.5"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::ERROR, Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(
                stderr.contains("non-integral operand"),
                "stderr = {stderr:?}",
            )
        });
    }

    #[test]
    fn too_many_operands() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("shift"),
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
}
