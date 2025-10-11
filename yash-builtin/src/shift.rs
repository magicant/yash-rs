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
//! This module implements the [`shift` built-in], which removes some positional
//! parameters.
//!
//! [`shift` built-in]: https://magicant.github.io/yash-rs/builtins/shift.html

use crate::common::report::{report_error, report_failure, syntax_error};
use crate::common::syntax::{Mode, parse_arguments};
use yash_env::Env;
use yash_env::builtin::Result;
use yash_env::semantics::Field;
use yash_syntax::source::Location;
use yash_syntax::source::pretty::{
    Footnote, FootnoteType, Report, ReportType, Snippet, Span, SpanRole, add_span,
};

pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO Support non-POSIX options
    let args = match parse_arguments(&[], Mode::with_env(env), args) {
        Ok((_options, operands)) => operands,
        Err(error) => return report_error(env, &error).await,
    };

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
    let len = params.values.len();
    if len < count {
        // Failure: cannot shift so many positional parameters
        let last_location = params.last_modified_location.clone();
        let report = report_for_too_large_operand(count, operand_location, len, &last_location);
        return report_failure(env, report).await;
    }

    params.values.drain(..count);
    params.last_modified_location = env.stack.current_builtin().map(|b| b.name.origin.clone());
    Result::default()
}

/// Constructs a report for the case where too many positional parameters are
/// requested to be shifted.
fn report_for_too_large_operand<'a>(
    count: usize,
    operand_location: Option<&'a Location>,
    len: usize,
    last_location: &'a Option<Location>,
) -> Report<'a> {
    let mut report = Report::new();
    report.r#type = ReportType::Error;
    report.title = "cannot shift positional parameters".into();

    match operand_location {
        None => report.footnotes.push(Footnote {
            r#type: FootnoteType::Note,
            label: "there are no positional parameters".into(),
        }),
        Some(location) => {
            report.snippets = Snippet::with_primary_span(
                location,
                format!(
                    "requested to shift {} but there {} only {}",
                    count,
                    if len == 1 { "is" } else { "are" },
                    len,
                )
                .into(),
            );
        }
    }

    if let Some(location) = &last_location {
        add_span(
            &location.code,
            Span {
                range: location.byte_range(),
                role: SpanRole::Supplementary {
                    label: "positional parameters were last modified here".into(),
                },
            },
            &mut report.snippets,
        );
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use std::vec;
    use yash_env::VirtualSystem;
    use yash_env::semantics::Divert;
    use yash_env::semantics::ExitStatus;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env_test_helper::assert_stderr;
    use yash_syntax::source::Location;

    #[test]
    fn shifting_without_operand() {
        let mut env = Env::new_virtual();
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("shift"),
            is_special: true,
        }));
        env.variables.positional_params_mut().values =
            vec!["1".to_string(), "2".to_string(), "3".to_string()];

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        assert_eq!(
            env.variables.positional_params().values,
            ["2".to_string(), "3".to_string()],
        );

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        assert_eq!(env.variables.positional_params().values, ["3".to_string()],);

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        let params = env.variables.positional_params();
        assert_eq!(params.values, [] as [String; 0]);
        assert_eq!(
            params.last_modified_location,
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
        env.variables.positional_params_mut().values = ["1", "2", "3", "4", "5", "6", "7"]
            .into_iter()
            .map(Into::into)
            .collect();

        let args = Field::dummies(["2"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        assert_eq!(
            env.variables.positional_params().values,
            [
                "3".to_string(),
                "4".to_string(),
                "5".to_string(),
                "6".to_string(),
                "7".to_string(),
            ],
        );

        let args = Field::dummies(["3"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        assert_eq!(
            env.variables.positional_params().values,
            ["6".to_string(), "7".to_string(),],
        );

        let args = Field::dummies(["2"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::default());
        assert_eq!(env.variables.positional_params().values, [] as [String; 0]);
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
        env.variables.positional_params_mut().values =
            vec!["1".to_string(), "2".to_string(), "3".to_string()];

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
