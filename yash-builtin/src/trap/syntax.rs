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

//! Command line argument parser for the trap built-in

use super::Command;
use crate::common::syntax::OptionOccurrence;
use itertools::Itertools;
use std::borrow::Cow;
use thiserror::Error;
use yash_env::semantics::Field;
use yash_env::trap::Action;
use yash_syntax::source::pretty::{Annotation, AnnotationType, Footer, MessageBase};

/// Error that may occur while [interpreting](interpret) command line arguments.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// The specified condition is not supported.
    #[error("unknown condition: {0}")]
    UnknownCondition(Field),

    /// An action is specified but no condition is specified.
    #[error("missing condition")]
    MissingCondition { action: Field },
}

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        match self {
            Error::UnknownCondition(_) => "cannot update trap",
            Error::MissingCondition { action: _ } => "trap condition is missing",
        }
        .into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        let (label, location) = match self {
            Error::UnknownCondition(field) => {
                (format!("unknown condition `{field}`").into(), &field.origin)
            }
            Error::MissingCondition { action } => (
                "trap action specified without condition".into(),
                &action.origin,
            ),
        };

        Annotation::new(AnnotationType::Error, label, location)
    }

    fn footers(&self) -> Vec<Footer> {
        match self {
            Error::UnknownCondition(_) => vec![],
            Error::MissingCondition { action } => vec![Footer {
                r#type: AnnotationType::Note,
                label: format!(
                    "the first operand `{action}` was not regarded as a condition \
                     because it was not an unsigned integer"
                )
                .into(),
            }],
        }
    }
}

/// Converts parsed command line arguments into a `Command`.
///
/// The result of [`parse_arguments`](crate::common::syntax::parse_arguments)
/// should be passed to this function.
///
/// On failure, returns a non-empty list of errors.
pub fn interpret(
    _options: Vec<OptionOccurrence>,
    operands: Vec<Field>,
) -> Result<Command, Vec<Error>> {
    let mut operands = operands.into_iter().peekable();

    // Parse the first operand as an action
    let action_field = operands
        .next_if(|field| !is_non_negative_integer(&field.value))
        .map(|field| {
            let action = match field.value.as_str() {
                "-" => Action::Default,
                "" => Action::Ignore,
                command => Action::Command(command.into()),
            };
            (action, field)
        });

    // Parse the remaining operands as conditions
    let (conditions, errors): (Vec<_>, Vec<_>) = operands
        .map(|operand| match operand.value.parse() {
            Ok(condition) => Ok((condition, operand)),
            Err(_) => Err(Error::UnknownCondition(operand)),
        })
        .partition_result();

    if !errors.is_empty() {
        Err(errors)
    } else {
        match (conditions.is_empty(), action_field) {
            (true, None) => Ok(Command::PrintAll),
            (true, Some((_, action))) => Err(vec![Error::MissingCondition { action }]),
            (false, action) => {
                let action = action.map(|(action, _)| action).unwrap_or_default();
                Ok(Command::SetAction { action, conditions })
            }
        }
    }
}

fn is_non_negative_integer(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::super::CondSpec;
    use super::*;
    use yash_env::signal::Name;

    #[test]
    fn print_all_for_no_operands() {
        let result = interpret(vec![], vec![]);
        assert_eq!(result, Ok(Command::PrintAll));
    }

    #[test]
    fn default_action_with_one_condition() {
        let result = interpret(vec![], Field::dummies(["-", "INT"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(CondSpec::SignalName(Name::Int), Field::dummy("INT"))]
            })
        );
    }

    #[test]
    fn ignore_action() {
        let result = interpret(vec![], Field::dummies(["", "INT"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Ignore,
                conditions: vec![(CondSpec::SignalName(Name::Int), Field::dummy("INT"))]
            })
        );
    }

    #[test]
    fn command_action() {
        let result = interpret(vec![], Field::dummies(["echo", "INT"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Command("echo".into()),
                conditions: vec![(CondSpec::SignalName(Name::Int), Field::dummy("INT"))]
            })
        );
    }

    #[test]
    fn action_with_multiple_conditions() {
        let result = interpret(vec![], Field::dummies(["-", "HUP", "2", "TERM"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![
                    (CondSpec::SignalName(Name::Hup), Field::dummy("HUP")),
                    (CondSpec::Number(2), Field::dummy("2")),
                    (CondSpec::SignalName(Name::Term), Field::dummy("TERM")),
                ]
            })
        );
    }

    #[test]
    fn action_with_different_signal_name_conditions() {
        let result = interpret(vec![], Field::dummies(["", "HUP"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Ignore,
                conditions: vec![(CondSpec::SignalName(Name::Hup), Field::dummy("HUP"))]
            })
        );

        let result = interpret(vec![], Field::dummies(["", "QUIT"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Ignore,
                conditions: vec![(CondSpec::SignalName(Name::Quit), Field::dummy("QUIT"))]
            })
        );
    }

    #[test]
    fn action_with_signal_number_condition() {
        let result = interpret(vec![], Field::dummies(["-", "1"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(CondSpec::Number(1), Field::dummy("1"))]
            })
        );
    }

    #[test]
    fn action_with_named_exit_condition() {
        let result = interpret(vec![], Field::dummies(["-", "EXIT"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(CondSpec::Exit, Field::dummy("EXIT"))]
            })
        );
    }

    #[test]
    fn action_with_numeric_exit_condition() {
        let result = interpret(vec![], Field::dummies(["-", "0"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(CondSpec::Number(0), Field::dummy("0"))]
            })
        );
    }

    #[test]
    fn action_with_unknown_conditions() {
        let result = interpret(vec![], Field::dummies(["-", "FOOBAR", "INT", "9999999999"]));
        assert_eq!(
            result,
            Err(vec![
                Error::UnknownCondition(Field::dummy("FOOBAR")),
                Error::UnknownCondition(Field::dummy("9999999999")),
            ])
        );
    }

    #[test]
    fn signal_number_condition_without_action() {
        let result = interpret(vec![], Field::dummies(["1"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(CondSpec::Number(1), Field::dummy("1"))]
            })
        );
    }

    #[test]
    fn numeric_exit_condition_without_action() {
        let result = interpret(vec![], Field::dummies(["0"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(CondSpec::Number(0), Field::dummy("0"))]
            })
        );
    }

    #[test]
    fn action_that_looks_like_negative_number() {
        let result = interpret(vec![], Field::dummies(["-1", "0"]));
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Command("-1".into()),
                conditions: vec![(CondSpec::Number(0), Field::dummy("0"))]
            })
        );
    }

    #[test]
    fn missing_condition() {
        let result = interpret(vec![], Field::dummies(["echo"]));
        assert_eq!(
            result,
            Err(vec![Error::MissingCondition {
                action: Field::dummy("echo")
            }])
        );
    }
}
