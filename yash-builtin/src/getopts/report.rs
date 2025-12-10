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

//! Reporting the result to the environment
//!
//! See [`model::Result::report`] for the implementation of the reporting
//! logic.

use super::indexes_to_optind;
use super::model;
use super::verify::GetoptsState;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::source::Location;
use yash_env::source::pretty::{
    Footnote, FootnoteType, Report, ReportType, Snippet, Span, SpanRole, add_span,
};
use yash_env::variable::AssignError;
use yash_env::variable::OPTARG;
use yash_env::variable::OPTIND;
use yash_env::variable::Scope;
use yash_env::variable::UnsetError;
use yash_env::variable::Value;

/// Error in reporting the result to the environment
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum Error {
    /// Variable name not acceptable
    #[error("invalid variable name")]
    InvalidVariableName { name: Field },

    /// Error in assigning to a read-only variable
    #[error("cannot update read-only variable `{name}`")]
    AssignReadOnlyError {
        /// Name of the variable that was being assigned
        name: String,
        /// Value that was being assigned
        new_value: Value,
        /// Location of the failed assignment
        assigned_location: Option<Location>,
        /// Location where the variable was made read-only
        read_only_location: Location,
    },

    /// Error in unsetting a read-only variable
    #[error("cannot unset read-only variable `{name}`")]
    UnsetReadOnlyError {
        /// Name of the variable that was being unset
        name: String,
        /// Location where the variable was made read-only
        read_only_location: Location,
    },
}

impl From<UnsetError<'_>> for Error {
    fn from(e: UnsetError) -> Self {
        Error::UnsetReadOnlyError {
            name: e.name.to_owned(),
            read_only_location: e.read_only_location.clone(),
        }
    }
}

impl Error {
    #[must_use]
    fn with_name_and_assign_error(name: String, e: AssignError) -> Self {
        Error::AssignReadOnlyError {
            name,
            new_value: e.new_value,
            assigned_location: e.assigned_location,
            read_only_location: e.read_only_location,
        }
    }

    /// Converts this error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();

        match self {
            Self::InvalidVariableName { name } => {
                report.snippets = Snippet::with_primary_span(
                    &name.origin,
                    format!("variable name {:?} is not valid", name.value).into(),
                );
            }

            Self::AssignReadOnlyError {
                name,
                new_value,
                assigned_location,
                read_only_location,
            } => {
                let label = format!(
                    "the built-in needs to update the variable to {}",
                    new_value.quote()
                )
                .into();
                if let Some(location) = assigned_location {
                    add_span(
                        &location.code,
                        Span {
                            range: location.byte_range(),
                            role: SpanRole::Supplementary { label },
                        },
                        &mut report.snippets,
                    );
                } else {
                    report.footnotes.push(Footnote {
                        r#type: FootnoteType::Note,
                        label,
                    });
                }

                add_span(
                    &read_only_location.code,
                    Span {
                        range: read_only_location.byte_range(),
                        role: SpanRole::Supplementary {
                            label: format!("`{name}` was made read-only here").into(),
                        },
                    },
                    &mut report.snippets,
                );
            }

            Self::UnsetReadOnlyError {
                name,
                read_only_location,
            } => {
                add_span(
                    &read_only_location.code,
                    Span {
                        range: read_only_location.byte_range(),
                        role: SpanRole::Supplementary {
                            label: format!("`{name}` was made read-only here").into(),
                        },
                    },
                    &mut report.snippets,
                );
            }
        }

        report
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(e: &'a Error) -> Self {
        e.to_report()
    }
}

impl model::Result {
    /// Updates variables to reflect the result and returns an error message to
    /// be printed to the standard error.
    ///
    /// The `colon` parameter must be `true` if and only if the option
    /// specification starts with a colon (`:`).
    ///
    /// This method updates `$OPTIND`, `$OPTARG`, and the variable named by
    /// `var_name` to reflect the result. If the result is an option error and
    /// `colon` is `false`, this method returns a non-empty error message.
    /// If the result is an option successfully parsed, this method returns an
    /// empty string. If the result is not an option, this method returns
    /// `Ok(None)`. In case of an error in updating the variables, this method
    /// returns an error value.
    ///
    /// If `env.getopts_state` is `Some`, this method also updates the `optind`
    /// field of the state to match the new `$OPTIND` value.
    pub fn report<S>(
        self,
        env: &mut Env<S>,
        colon: bool,
        var_name: Field,
    ) -> Result<Option<String>, Error> {
        if var_name.value.contains('=') {
            return Err(Error::InvalidVariableName { name: var_name });
        }

        let location = env.stack.current_builtin().map(|b| b.name.origin.clone());

        let (var_value, optarg, message) = match self.option {
            None => ('?', None, None),

            Some(occurrence) => match occurrence.error {
                None => (occurrence.option, occurrence.argument, Some(String::new())),

                Some(model::Error::UnknownOption) if colon => (
                    '?',
                    Some(occurrence.option.to_string()),
                    Some(String::new()),
                ),

                Some(model::Error::MissingArgument) if colon => (
                    ':',
                    Some(occurrence.option.to_string()),
                    Some(String::new()),
                ),

                Some(model::Error::UnknownOption) => {
                    let message =
                        format!("{}: invalid option `-{}`\n", env.arg0, occurrence.option);
                    ('?', None, Some(message))
                }

                Some(model::Error::MissingArgument) => {
                    let message = format!(
                        "{}: option `-{}` requires an argument\n",
                        env.arg0, occurrence.option
                    );
                    ('?', None, Some(message))
                }
            },
        };

        env.get_or_create_variable(var_name.value.clone(), Scope::Global)
            .assign(var_value.to_string(), var_name.origin)
            .map_err(|e| Error::with_name_and_assign_error(var_name.value.clone(), e))?;

        if let Some(value) = optarg {
            env.get_or_create_variable(OPTARG, Scope::Global)
                .assign(value, location.clone())
                .map_err(|e| Error::with_name_and_assign_error(OPTARG.to_string(), e))?;
        } else {
            env.variables.unset(OPTARG, Scope::Global)?;
        }

        let optind = indexes_to_optind(self.next_arg_index, self.next_char_index);
        env.get_or_create_variable(OPTIND, Scope::Global)
            .assign(optind.clone(), location)
            .map_err(|e| Error::with_name_and_assign_error(OPTIND.to_string(), e))?;

        if let Some(state) = env.any.get_mut::<GetoptsState>() {
            state.optind = optind;
        }

        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::super::verify::Origin;
    use super::*;
    use assert_matches::assert_matches;
    use std::num::NonZeroUsize;
    use yash_env::source::Location;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_env::variable::Value;
    use yash_env::variable::Variable;

    fn non_zero(i: usize) -> NonZeroUsize {
        NonZeroUsize::new(i).unwrap()
    }

    fn env_with_dummy_arg0_and_optarg() -> Env<VirtualSystem> {
        let mut env = Env::new_virtual();
        env.arg0 = "some/arg0".to_string();
        env.get_or_create_variable(OPTARG, Scope::Global)
            .assign("DUMMY", None)
            .unwrap();
        env
    }

    fn assert_variable_scalar<S>(env: &Env<S>, name: &str, value: &str) {
        assert_matches!(
            &env.variables.get(name),
            Some(Variable { value: Some(Value::Scalar(s)), .. }) if s == value,
            "expected ${name} == {value:?}",
        );
    }

    fn assert_variable_none<S>(env: &Env<S>, name: &str) {
        assert_matches!(env.variables.get(name), None, "expected ${name} unset");
    }

    #[test]
    fn report_standard_result() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: None,
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        let report = result.report(&mut env, false, Field::dummy("opt_var"));

        assert_eq!(report, Ok(Some(String::new())));
        assert_variable_scalar(&env, "opt_var", "a");
        assert_variable_scalar(&env, OPTIND, "2");
        assert_variable_none(&env, OPTARG);
    }

    #[test]
    fn name_and_location_of_reported_variable() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'b',
                argument: None,
                error: None,
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };
        let var_name = Field {
            value: "RESULT_VARIABLE".to_string(),
            origin: Location::dummy("my dummy location"),
        };

        _ = result.report(&mut env, false, var_name.clone());

        let var = env.variables.get(&var_name.value).unwrap();
        assert_matches!(&var.value, Some(Value::Scalar(s)) if s == "b");
        assert_eq!(var.last_assigned_location, Some(var_name.origin));
    }

    #[test]
    fn location_of_reported_optind() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let location = Location::dummy("invocation location");
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field {
                value: "my_builtin".to_string(),
                origin: location.clone(),
            },
            is_special: false,
        }));
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: None,
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        _ = result.report(&mut env, false, Field::dummy("opt_var"));

        let optind = env.variables.get(OPTIND).unwrap();
        assert_matches!(&optind.value, Some(Value::Scalar(s)) if s == "2");
        assert_eq!(optind.last_assigned_location, Some(location));
    }

    #[test]
    fn location_of_reported_optarg() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let location = Location::dummy("invocation location");
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field {
                value: "my_builtin".to_string(),
                origin: location.clone(),
            },
            is_special: false,
        }));
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: Some("some argument".to_string()),
                error: None,
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        _ = result.report(&mut env, false, Field::dummy("opt_var"));

        let optind = env.variables.get(OPTARG).unwrap();
        assert_matches!(&optind.value, Some(Value::Scalar(s)) if s == "some argument");
        assert_eq!(optind.last_assigned_location, Some(location));
    }

    #[test]
    fn report_updates_optind_of_getopts_state() {
        let mut env = env_with_dummy_arg0_and_optarg();
        env.any.insert(Box::new(GetoptsState {
            args: vec!["-a".to_string(), "-b".to_string()],
            origin: Origin::DirectArgs,
            optind: "1".to_string(),
        }));
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: None,
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        _ = result.report(&mut env, false, Field::dummy("opt_var"));

        assert_eq!(env.any.get::<GetoptsState>().unwrap().optind, "2");
    }

    #[test]
    fn report_with_option_argument() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: Some("foo".to_string()),
                error: None,
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        let report = result.report(&mut env, false, Field::dummy("opt_var"));

        assert_eq!(report, Ok(Some(String::new())));
        assert_variable_scalar(&env, "opt_var", "a");
        assert_variable_scalar(&env, OPTIND, "2");
        assert_variable_scalar(&env, OPTARG, "foo");
    }

    #[test]
    fn report_with_next_char_index_other_than_one() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: None,
            }),
            next_arg_index: non_zero(4),
            next_char_index: non_zero(3),
        };

        let report = result.report(&mut env, false, Field::dummy("opt_var"));

        assert_eq!(report, Ok(Some(String::new())));
        assert_variable_scalar(&env, "opt_var", "a");
        assert_variable_scalar(&env, OPTIND, "4:3");
        assert_variable_none(&env, OPTARG);
    }

    #[test]
    fn report_unknown_option_with_colon() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: Some(model::Error::UnknownOption),
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        let report = result.report(&mut env, true, Field::dummy("opt_var"));

        assert_eq!(report, Ok(Some(String::new())));
        assert_variable_scalar(&env, "opt_var", "?");
        assert_variable_scalar(&env, OPTIND, "2");
        assert_variable_scalar(&env, OPTARG, "a");
    }

    #[test]
    fn report_unknown_option_without_colon() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: Some(model::Error::UnknownOption),
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        let report = result.report(&mut env, false, Field::dummy("opt_var"));

        let message = report.unwrap().unwrap();
        assert!(message.starts_with(&env.arg0), "message = {message:?}");
        assert!(message.contains("-a"), "message = {message:?}");
        assert!(message.contains("invalid"), "message = {message:?}");
        assert_variable_scalar(&env, "opt_var", "?");
        assert_variable_scalar(&env, OPTIND, "2");
        assert_variable_none(&env, OPTARG);
    }

    #[test]
    fn report_missing_argument_with_colon() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: Some(model::Error::MissingArgument),
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        let report = result.report(&mut env, true, Field::dummy("opt_var"));

        assert_eq!(report, Ok(Some(String::new())));
        assert_variable_scalar(&env, "opt_var", ":");
        assert_variable_scalar(&env, OPTIND, "2");
        assert_variable_scalar(&env, OPTARG, "a");
    }

    #[test]
    fn report_missing_argument_without_colon() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: Some(model::Error::MissingArgument),
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        let report = result.report(&mut env, false, Field::dummy("opt_var"));

        let message = report.unwrap().unwrap();
        assert!(message.starts_with(&env.arg0), "message = {message:?}");
        assert!(message.contains("-a"), "message = {message:?}");
        assert!(message.contains("argument"), "message = {message:?}");
        assert_variable_scalar(&env, "opt_var", "?");
        assert_variable_scalar(&env, OPTIND, "2");
        assert_variable_none(&env, OPTARG);
    }

    #[test]
    fn report_with_invalid_variable_name() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: None,
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        let name = Field::dummy("opt=var");
        let report = result.report(&mut env, false, name.clone());

        assert_eq!(report, Err(Error::InvalidVariableName { name }));
    }

    #[test]
    fn report_with_readonly_optind() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let invocation_location = Location::dummy("invocation location");
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field {
                value: "my_builtin".to_string(),
                origin: invocation_location.clone(),
            },
            is_special: false,
        }));
        let mut optind = env.get_or_create_variable(OPTIND, Scope::Global);
        let read_only_location = Location::dummy("read-only location");
        optind.make_read_only(read_only_location.clone());
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: None,
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        let report = result.report(&mut env, false, Field::dummy("opt_var"));

        assert_eq!(
            report,
            Err(Error::AssignReadOnlyError {
                name: OPTIND.to_string(),
                new_value: Value::scalar("2"),
                assigned_location: Some(invocation_location),
                read_only_location,
            })
        );
    }

    #[test]
    fn report_with_readonly_optarg_to_be_assigned() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let invocation_location = Location::dummy("invocation location");
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field {
                value: "my_builtin".to_string(),
                origin: invocation_location.clone(),
            },
            is_special: false,
        }));
        let mut optarg = env.get_or_create_variable(OPTARG, Scope::Global);
        let read_only_location = Location::dummy("read-only location");
        optarg.make_read_only(read_only_location.clone());
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: Some("some argument".to_string()),
                error: None,
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        let report = result.report(&mut env, false, Field::dummy("opt_var"));

        assert_eq!(
            report,
            Err(Error::AssignReadOnlyError {
                name: OPTARG.to_string(),
                new_value: Value::scalar("some argument"),
                assigned_location: Some(invocation_location),
                read_only_location,
            })
        );
    }

    #[test]
    fn report_with_readonly_optarg_to_be_unset() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let mut optarg = env.get_or_create_variable(OPTARG, Scope::Global);
        let read_only_location = Location::dummy("read-only location");
        optarg.make_read_only(read_only_location.clone());
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: None,
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };

        let report = result.report(&mut env, false, Field::dummy("opt_var"));

        assert_eq!(
            report,
            Err(Error::UnsetReadOnlyError {
                name: OPTARG.to_string(),
                read_only_location,
            })
        );
    }

    #[test]
    fn report_with_readonly_option_var() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let mut optarg = env.get_or_create_variable("var", Scope::Global);
        let read_only_location = Location::dummy("read-only location");
        optarg.make_read_only(read_only_location.clone());
        let result = model::Result {
            option: Some(model::OptionOccurrence {
                option: 'a',
                argument: None,
                error: None,
            }),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
        };
        let var_location = Location::dummy("var location");
        let var = Field {
            value: "var".to_string(),
            origin: var_location.clone(),
        };

        let report = result.report(&mut env, false, var);

        assert_eq!(
            report,
            Err(Error::AssignReadOnlyError {
                name: "var".to_string(),
                new_value: Value::scalar("a"),
                assigned_location: Some(var_location),
                read_only_location,
            })
        );
    }
}
