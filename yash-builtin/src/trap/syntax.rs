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
use crate::common::syntax::{OptionOccurrence, OptionSpec};
use itertools::Itertools;
use thiserror::Error;
use yash_env::semantics::Field;
use yash_env::signal::RawNumber;
use yash_env::source::pretty::{Footnote, FootnoteType, Report, ReportType, Snippet};
use yash_env::system::Signals;
use yash_env::trap::{Action, Condition};

/// Command line options for the trap built-in
pub const OPTION_SPECS: &[OptionSpec] = &[OptionSpec::new().short('p').long("print")];

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

impl Error {
    /// Converts the error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        match self {
            Self::UnknownCondition(field) => {
                report.title = "unknown condition".into();
                report.snippets = Snippet::with_primary_span(
                    &field.origin,
                    format!("unknown condition `{field}`").into(),
                );
            }
            Self::MissingCondition { action } => {
                report.title = "trap condition is missing".into();
                report.snippets = Snippet::with_primary_span(
                    &action.origin,
                    "trap action specified without condition".into(),
                );
                report.footnotes.push(Footnote {
                    r#type: FootnoteType::Note,
                    label: format!(
                        "the first operand `{action}` was not regarded as a condition \
                         because it was not an unsigned integer"
                    )
                    .into(),
                });
            }
        }
        report
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

/// Parses a single condition from a command line operand.
///
/// On success, returns the parsed `Condition` and the original `Field`.
/// On failure, returns `Error::UnknownCondition`.
///
/// A condition can be `0` or `EXIT` for [`Condition::Exit`], or a signal
/// name/number for [`Condition::Signal`].
fn parse_condition<S: Signals>(field: Field, system: &S) -> Result<(Condition, Field), Error> {
    // TODO Case-insensitive parse
    // TODO Allow SIG prefix
    match field.value.parse::<RawNumber>() {
        Ok(0) => Ok((Condition::Exit, field)),
        Ok(number) => match system.to_signal_number(number) {
            Some(number) => Ok((Condition::Signal(number), field)),
            None => Err(Error::UnknownCondition(field)),
        },
        Err(_) if field.value == "EXIT" => Ok((Condition::Exit, field)),
        Err(_) => match system.str2sig(&field.value) {
            Some(number) => Ok((Condition::Signal(number), field)),
            None => Err(Error::UnknownCondition(field)),
        },
    }
}

/// Converts parsed command line arguments into a `Command`.
///
/// The result of [`parse_arguments`](crate::common::syntax::parse_arguments)
/// should be passed to this function.
///
/// On failure, returns a non-empty list of errors.
///
/// If a given option occurrence is not recognized, it is ignored.
pub fn interpret<S: Signals>(
    options: Vec<OptionOccurrence>,
    operands: Vec<Field>,
    system: &S,
) -> Result<Command, Vec<Error>> {
    let mut print = false;
    let mut operands = operands.into_iter().peekable();

    // Parse options
    for option in options {
        if option.spec.get_short() == Some('p') {
            print = true;
        }
    }

    // Parse the first operand as an action
    let action_field = operands
        .next_if(|field| !print && !is_non_negative_integer(&field.value))
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
        .map(|operand| parse_condition(operand, system))
        .partition_result();

    if !errors.is_empty() {
        Err(errors)
    } else if print {
        if conditions.is_empty() {
            Ok(Command::PrintAll {
                include_default: true,
            })
        } else {
            Ok(Command::Print { conditions })
        }
    } else {
        match (conditions.is_empty(), action_field) {
            (true, None) => Ok(Command::PrintAll {
                include_default: false,
            }),
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
    use super::*;
    use std::num::NonZero;
    use yash_env::signal::Number;
    use yash_env::source::Location;
    use yash_env::system::r#virtual::VirtualSystem;

    #[test]
    fn parse_condition_exit_numeric() {
        let system = VirtualSystem::new();
        let field = Field::dummy("0");
        let result = parse_condition(field.clone(), &system);
        assert_eq!(result, Ok((Condition::Exit, field)));
    }

    #[test]
    fn parse_condition_exit_named() {
        let system = VirtualSystem::new();
        let field = Field::dummy("EXIT");
        let result = parse_condition(field.clone(), &system);
        assert_eq!(result, Ok((Condition::Exit, field)));
    }

    #[test]
    fn parse_condition_signal_by_name() {
        let system = VirtualSystem::new();
        let field = Field::dummy("INT");
        let result = parse_condition(field.clone(), &system);
        assert_eq!(
            result,
            Ok((Condition::Signal(VirtualSystem::SIGINT), field))
        );
    }

    #[test]
    fn parse_condition_signal_by_number() {
        let system = VirtualSystem::new();
        let field = Field::dummy("2");
        let result = parse_condition(field.clone(), &system);
        assert_eq!(
            result,
            Ok((Condition::Signal(VirtualSystem::SIGINT), field))
        );
    }

    #[test]
    fn parse_condition_unknown_name() {
        let system = VirtualSystem::new();
        let field = Field::dummy("FOOBAR");
        let result = parse_condition(field.clone(), &system);
        assert_eq!(result, Err(Error::UnknownCondition(field)));
    }

    #[test]
    fn parse_condition_invalid_signal_number() {
        let system = VirtualSystem::new();
        let field = Field::dummy("9999999999");
        let result = parse_condition(field.clone(), &system);
        assert_eq!(result, Err(Error::UnknownCondition(field)));
    }

    #[test]
    fn parse_condition_negative_number() {
        let system = VirtualSystem::new();
        let field = Field::dummy("-1");
        let result = parse_condition(field.clone(), &system);
        assert_eq!(result, Err(Error::UnknownCondition(field)));
    }

    #[test]
    fn print_all_not_including_default() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], vec![], &system);
        assert_eq!(
            result,
            Ok(Command::PrintAll {
                include_default: false
            })
        );
    }

    #[test]
    fn print_all_including_default() {
        let system = VirtualSystem::new();
        let print = OptionOccurrence {
            spec: &OptionSpec::new().short('p').long("print"),
            location: Location::dummy("-p"),
            argument: None,
        };
        let result = interpret(vec![print], vec![], &system);
        assert_eq!(
            result,
            Ok(Command::PrintAll {
                include_default: true
            })
        );
    }

    #[test]
    fn print_one_condition() {
        let system = VirtualSystem::new();
        let print = OptionOccurrence {
            spec: &OptionSpec::new().short('p').long("print"),
            location: Location::dummy("-p"),
            argument: None,
        };
        let result = interpret(vec![print], Field::dummies(["INT"]), &system);
        assert_eq!(
            result,
            Ok(Command::Print {
                conditions: vec![(
                    Condition::Signal(VirtualSystem::SIGINT),
                    Field::dummy("INT")
                )]
            })
        )
    }

    #[test]
    fn print_multiple_conditions() {
        let system = VirtualSystem::new();
        let print = OptionOccurrence {
            spec: &OptionSpec::new().short('p').long("print"),
            location: Location::dummy("-p"),
            argument: None,
        };
        let result = interpret(
            vec![print],
            Field::dummies(["HUP", "EXIT", "QUIT"]),
            &system,
        );
        assert_eq!(
            result,
            Ok(Command::Print {
                conditions: vec![
                    (
                        Condition::Signal(VirtualSystem::SIGHUP),
                        Field::dummy("HUP")
                    ),
                    (Condition::Exit, Field::dummy("EXIT")),
                    (
                        Condition::Signal(VirtualSystem::SIGQUIT),
                        Field::dummy("QUIT")
                    ),
                ]
            })
        )
    }

    #[test]
    fn default_action_with_one_condition() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["-", "INT"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(
                    Condition::Signal(VirtualSystem::SIGINT),
                    Field::dummy("INT")
                )]
            })
        );
    }

    #[test]
    fn ignore_action() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["", "INT"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Ignore,
                conditions: vec![(
                    Condition::Signal(VirtualSystem::SIGINT),
                    Field::dummy("INT")
                )]
            })
        );
    }

    #[test]
    fn command_action() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["echo", "INT"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Command("echo".into()),
                conditions: vec![(
                    Condition::Signal(VirtualSystem::SIGINT),
                    Field::dummy("INT")
                )]
            })
        );
    }

    #[test]
    fn action_with_multiple_conditions() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["-", "HUP", "2", "TERM"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![
                    (
                        Condition::Signal(VirtualSystem::SIGHUP),
                        Field::dummy("HUP")
                    ),
                    (
                        Condition::Signal(Number::from_raw_unchecked(NonZero::new(2).unwrap())),
                        Field::dummy("2")
                    ),
                    (
                        Condition::Signal(VirtualSystem::SIGTERM),
                        Field::dummy("TERM")
                    ),
                ]
            })
        );
    }

    #[test]
    fn action_with_different_signal_name_conditions() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["", "HUP"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Ignore,
                conditions: vec![(
                    Condition::Signal(VirtualSystem::SIGHUP),
                    Field::dummy("HUP")
                )]
            })
        );

        let result = interpret(vec![], Field::dummies(["", "QUIT"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Ignore,
                conditions: vec![(
                    Condition::Signal(VirtualSystem::SIGQUIT),
                    Field::dummy("QUIT")
                )]
            })
        );
    }

    #[test]
    fn action_with_signal_number_condition() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["-", "1"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(
                    Condition::Signal(Number::from_raw_unchecked(NonZero::new(1).unwrap())),
                    Field::dummy("1")
                )]
            })
        );
    }

    #[test]
    fn action_with_named_exit_condition() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["-", "EXIT"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(Condition::Exit, Field::dummy("EXIT"))]
            })
        );
    }

    #[test]
    fn action_with_numeric_exit_condition() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["-", "0"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(Condition::Exit, Field::dummy("0"))]
            })
        );
    }

    #[test]
    fn action_with_unknown_conditions() {
        let system = VirtualSystem::new();
        let result = interpret(
            vec![],
            Field::dummies(["-", "FOOBAR", "INT", "9999999999"]),
            &system,
        );
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
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["1"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(
                    Condition::Signal(Number::from_raw_unchecked(NonZero::new(1).unwrap())),
                    Field::dummy("1")
                )]
            })
        );
    }

    #[test]
    fn numeric_exit_condition_without_action() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["0"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Default,
                conditions: vec![(Condition::Exit, Field::dummy("0"))]
            })
        );
    }

    #[test]
    fn action_that_looks_like_negative_number() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["-1", "0"]), &system);
        assert_eq!(
            result,
            Ok(Command::SetAction {
                action: Action::Command("-1".into()),
                conditions: vec![(Condition::Exit, Field::dummy("0"))]
            })
        );
    }

    #[test]
    fn missing_condition() {
        let system = VirtualSystem::new();
        let result = interpret(vec![], Field::dummies(["echo"]), &system);
        assert_eq!(
            result,
            Err(vec![Error::MissingCondition {
                action: Field::dummy("echo")
            }])
        );
    }
}
