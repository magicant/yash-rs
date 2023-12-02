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

use super::indexes_to_optind;
use super::model;
use thiserror::Error;
use yash_env::semantics::Field;
use yash_env::variable::AssignError;
use yash_env::variable::Scope;
use yash_env::variable::UnsetError;
use yash_env::variable::Value;
use yash_env::Env;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;

/// Error in reporting the result to the environment
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum Error {
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

    /// Converts this error to a printable message.
    pub fn to_message(&self) -> Message {
        let mut annotations = Vec::new();

        match self {
            Error::AssignReadOnlyError {
                name,
                new_value,
                assigned_location,
                read_only_location,
            } => {
                if let Some(location) = assigned_location {
                    annotations.push(Annotation::new(
                        AnnotationType::Info,
                        format!(
                            "the built-in needs to update the variable to `{}`",
                            new_value.quote()
                        )
                        .into(),
                        location,
                    ));
                }

                annotations.push(Annotation::new(
                    AnnotationType::Info,
                    format!("`{name}` was made read-only here").into(),
                    read_only_location,
                ));
            }

            Error::UnsetReadOnlyError {
                name,
                read_only_location,
            } => {
                annotations.push(Annotation::new(
                    AnnotationType::Info,
                    format!("`{name}` was made read-only here").into(),
                    read_only_location,
                ));
            }
        }

        Message {
            r#type: AnnotationType::Error,
            title: self.to_string().into(),
            annotations,
        }
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
    /// `var_name` to reflect the result. If the result is an error and `colon`
    /// is `false`, this method returns an error message. Otherwise, this method
    /// returns an empty string.
    pub fn report(self, env: &mut Env, colon: bool, var_name: Field) -> Result<String, Error> {
        let location = env.stack.current_builtin().map(|b| b.name.origin.clone());

        let (var_value, optarg, message) = match self.error {
            None => (self.option.to_string(), self.argument, String::new()),

            Some(model::Error::UnknownOption) if colon => (
                "?".to_string(),
                Some(self.option.to_string()),
                String::new(),
            ),

            Some(model::Error::MissingArgument) if colon => (
                ":".to_string(),
                Some(self.option.to_string()),
                String::new(),
            ),

            Some(model::Error::UnknownOption) => {
                let message = format!("{}: invalid option `-{}`\n", env.arg0, self.option);
                ("?".to_string(), None, message)
            }

            Some(model::Error::MissingArgument) => {
                let message = format!(
                    "{}: option `-{}` requires an argument\n",
                    env.arg0, self.option
                );
                ("?".to_string(), None, message)
            }
        };

        env.get_or_create_variable(var_name.value.clone(), Scope::Global)
            .assign(var_value, var_name.origin)
            .map_err(|e| Error::with_name_and_assign_error(var_name.value.clone(), e))?;

        if let Some(value) = optarg {
            env.get_or_create_variable("OPTARG", Scope::Global)
                .assign(value, location.clone())
                .map_err(|e| Error::with_name_and_assign_error("OPTARG".to_string(), e))?;
        } else {
            env.variables.unset(Scope::Global, "OPTARG")?;
        }

        let optind = indexes_to_optind(self.next_arg_index, self.next_char_index);
        env.get_or_create_variable("OPTIND", Scope::Global)
            .assign(optind, location)
            .map_err(|e| Error::with_name_and_assign_error("OPTIND".to_string(), e))?;

        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::num::NonZeroUsize;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env::variable::Value;
    use yash_env::variable::Variable;
    use yash_syntax::source::Location;

    fn non_zero(i: usize) -> NonZeroUsize {
        NonZeroUsize::new(i).unwrap()
    }

    fn env_with_dummy_arg0_and_optarg() -> Env {
        let mut env = Env::new_virtual();
        env.arg0 = "some/arg0".to_string();
        env.get_or_create_variable("OPTARG", Scope::Global)
            .assign("DUMMY", None)
            .unwrap();
        env
    }

    fn assert_variable_scalar(env: &Env, name: &str, value: &str) {
        assert_matches!(
            &env.variables.get(name),
            Some(Variable { value: Some(Value::Scalar(s)), .. }) if s == value,
            "expected ${name} == {value:?}",
        );
    }

    fn assert_variable_none(env: &Env, name: &str) {
        assert_matches!(env.variables.get(name), None, "expected ${name} unset");
    }

    #[test]
    fn report_standard_result() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: 'a',
            argument: None,
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: None,
        };

        let message = result
            .report(&mut env, false, Field::dummy("opt_var"))
            .unwrap();

        assert_eq!(message, "");
        assert_variable_scalar(&env, "opt_var", "a");
        assert_variable_scalar(&env, "OPTIND", "2");
        assert_variable_none(&env, "OPTARG");
    }

    #[test]
    fn name_and_location_of_reported_variable() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: 'b',
            argument: None,
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: None,
        };
        let var_name = Field {
            value: "RESULT_VARIABLE".to_string(),
            origin: Location::dummy("my dummy location"),
        };

        let _message = result.report(&mut env, false, var_name.clone()).unwrap();

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
            option: 'a',
            argument: None,
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: None,
        };

        let _message = result
            .report(&mut env, false, Field::dummy("opt_var"))
            .unwrap();

        let optind = env.variables.get("OPTIND").unwrap();
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
            option: 'a',
            argument: Some("some argument".to_string()),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: None,
        };

        let _message = result
            .report(&mut env, false, Field::dummy("opt_var"))
            .unwrap();

        let optind = env.variables.get("OPTARG").unwrap();
        assert_matches!(&optind.value, Some(Value::Scalar(s)) if s == "some argument");
        assert_eq!(optind.last_assigned_location, Some(location));
    }

    #[test]
    fn report_with_option_argument() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: 'a',
            argument: Some("foo".to_string()),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: None,
        };

        let message = result
            .report(&mut env, false, Field::dummy("opt_var"))
            .unwrap();

        assert_eq!(message, "");
        assert_variable_scalar(&env, "opt_var", "a");
        assert_variable_scalar(&env, "OPTIND", "2");
        assert_variable_scalar(&env, "OPTARG", "foo");
    }

    #[test]
    fn report_with_next_char_index_other_than_one() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: 'a',
            argument: None,
            next_arg_index: non_zero(4),
            next_char_index: non_zero(3),
            error: None,
        };

        let message = result
            .report(&mut env, false, Field::dummy("opt_var"))
            .unwrap();

        assert_eq!(message, "");
        assert_variable_scalar(&env, "opt_var", "a");
        assert_variable_scalar(&env, "OPTIND", "4:3");
        assert_variable_none(&env, "OPTARG");
    }

    #[test]
    fn report_unknown_option_with_colon() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: 'a',
            argument: None,
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: Some(model::Error::UnknownOption),
        };

        let message = result
            .report(&mut env, true, Field::dummy("opt_var"))
            .unwrap();

        assert_eq!(message, "");
        assert_variable_scalar(&env, "opt_var", "?");
        assert_variable_scalar(&env, "OPTIND", "2");
        assert_variable_scalar(&env, "OPTARG", "a");
    }

    #[test]
    fn report_unknown_option_without_colon() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: 'a',
            argument: None,
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: Some(model::Error::UnknownOption),
        };

        let message = result
            .report(&mut env, false, Field::dummy("opt_var"))
            .unwrap();

        assert!(message.starts_with(&env.arg0), "message = {message:?}");
        assert!(message.contains("-a"), "message = {message:?}");
        assert!(message.contains("invalid"), "message = {message:?}");
        assert_variable_scalar(&env, "opt_var", "?");
        assert_variable_scalar(&env, "OPTIND", "2");
        assert_variable_none(&env, "OPTARG");
    }

    #[test]
    fn report_missing_argument_with_colon() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: 'a',
            argument: None,
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: Some(model::Error::MissingArgument),
        };

        let message = result
            .report(&mut env, true, Field::dummy("opt_var"))
            .unwrap();

        assert_eq!(message, "");
        assert_variable_scalar(&env, "opt_var", ":");
        assert_variable_scalar(&env, "OPTIND", "2");
        assert_variable_scalar(&env, "OPTARG", "a");
    }

    #[test]
    fn report_missing_argument_without_colon() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let result = model::Result {
            option: 'a',
            argument: None,
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: Some(model::Error::MissingArgument),
        };

        let message = result
            .report(&mut env, false, Field::dummy("opt_var"))
            .unwrap();

        assert!(message.starts_with(&env.arg0), "message = {message:?}");
        assert!(message.contains("-a"), "message = {message:?}");
        assert!(message.contains("argument"), "message = {message:?}");
        assert_variable_scalar(&env, "opt_var", "?");
        assert_variable_scalar(&env, "OPTIND", "2");
        assert_variable_none(&env, "OPTARG");
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
        let mut optind = env.get_or_create_variable("OPTIND", Scope::Global);
        let read_only_location = Location::dummy("read-only location");
        optind.make_read_only(read_only_location.clone());
        let result = model::Result {
            option: 'a',
            argument: None,
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: None,
        };

        let result = result.report(&mut env, false, Field::dummy("opt_var"));

        assert_eq!(
            result,
            Err(Error::AssignReadOnlyError {
                name: "OPTIND".to_string(),
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
        let mut optarg = env.get_or_create_variable("OPTARG", Scope::Global);
        let read_only_location = Location::dummy("read-only location");
        optarg.make_read_only(read_only_location.clone());
        let result = model::Result {
            option: 'a',
            argument: Some("some argument".to_string()),
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: None,
        };

        let result = result.report(&mut env, false, Field::dummy("opt_var"));

        assert_eq!(
            result,
            Err(Error::AssignReadOnlyError {
                name: "OPTARG".to_string(),
                new_value: Value::scalar("some argument"),
                assigned_location: Some(invocation_location),
                read_only_location,
            })
        );
    }

    #[test]
    fn report_with_readonly_optarg_to_be_unset() {
        let mut env = env_with_dummy_arg0_and_optarg();
        let mut optarg = env.get_or_create_variable("OPTARG", Scope::Global);
        let read_only_location = Location::dummy("read-only location");
        optarg.make_read_only(read_only_location.clone());
        let result = model::Result {
            option: 'a',
            argument: None,
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: None,
        };

        let result = result.report(&mut env, false, Field::dummy("opt_var"));

        assert_eq!(
            result,
            Err(Error::UnsetReadOnlyError {
                name: "OPTARG".to_string(),
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
            option: 'a',
            argument: None,
            next_arg_index: non_zero(2),
            next_char_index: non_zero(1),
            error: None,
        };
        let var_location = Location::dummy("var location");
        let var = Field {
            value: "var".to_string(),
            origin: var_location.clone(),
        };

        let result = result.report(&mut env, false, var);

        assert_eq!(
            result,
            Err(Error::AssignReadOnlyError {
                name: "var".to_string(),
                new_value: Value::scalar("a"),
                assigned_location: Some(var_location),
                read_only_location,
            })
        );
    }
}
