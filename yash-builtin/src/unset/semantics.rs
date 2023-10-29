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

//! Defines the behavior of the unset built-in.

use crate::common::arrange_message_and_divert;
use thiserror::Error;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
#[cfg(doc)]
use yash_env::system::SharedSystem;
use yash_env::variable::Scope::Global;
use yash_env::Env;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;

/// Error returned by [`unset_variables`].
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub struct UnsetVariablesError<'a> {
    /// The name of the read-only variable
    pub name: &'a Field,
    /// The location where the variable was made read-only
    pub read_only_location: Location,
}

impl std::fmt::Display for UnsetVariablesError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let error = yash_env::variable::UnsetError {
            name: &self.name.value,
            read_only_location: &self.read_only_location,
        };
        error.fmt(f)
    }
}

/// Unsets shell variables.
///
/// This function tries to unset all the variables named by `names`. Any error
/// for a variable is reported in the returned vector and the function continues
/// to unset the remaining variables.
///
/// TODO Allow unsetting local variables only.
pub fn unset_variables<'a>(
    env: &mut Env,
    names: &'a [Field],
) -> Result<(), Vec<UnsetVariablesError<'a>>> {
    let mut errors = Vec::new();
    for name in names {
        match env.variables.unset(Global, &name.value) {
            Ok(_) => (),
            Err(error) => errors.push(UnsetVariablesError {
                name,
                read_only_location: error.read_only_location.clone(),
            }),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Creates a message that describes the errors.
///
/// See [`arrange_message_and_divert`] for the second return value.
#[must_use]
pub fn unset_variables_error_message(
    env: &Env,
    errors: &[UnsetVariablesError],
) -> (String, yash_env::semantics::Result) {
    let annotations = errors
        .iter()
        .flat_map(|error| {
            [
                Annotation::new(
                    AnnotationType::Error,
                    error.to_string().into(),
                    &error.name.origin,
                ),
                Annotation::new(
                    AnnotationType::Info,
                    format!("variable `{}` was made read-only here", error.name).into(),
                    &error.read_only_location,
                ),
            ]
        })
        .collect();
    let message = Message {
        r#type: AnnotationType::Error,
        title: "cannot unset variable".into(),
        annotations,
    };
    arrange_message_and_divert(env, message)
}

/// Prints an error message to the standard error.
///
/// This function constructs a message with [`unset_variables_error_message`]
/// and prints it with [`SharedSystem::print_error`].
pub async fn report_variables_error(
    env: &mut Env,
    errors: &[UnsetVariablesError<'_>],
) -> crate::Result {
    let (message, divert) = unset_variables_error_message(env, errors);
    env.system.print_error(&message).await;
    crate::Result::with_exit_status_and_divert(ExitStatus::FAILURE, divert)
}

/// Error returned by [`unset_functions`].
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cannot unset read-only function `{name}`")]
pub struct UnsetFunctionsError<'a> {
    /// The name of the function
    pub name: &'a Field,
    /// The location where the function was made read-only
    pub read_only_location: Location,
}

/// Unsets shell functions.
///
/// This function tries to unset all the functions named by `names`. Any error
/// for a function is reported in the returned vector and the function continues
/// to unset the remaining functions.
pub fn unset_functions<'a>(
    env: &mut Env,
    names: &'a [Field],
) -> Result<(), Vec<UnsetFunctionsError<'a>>> {
    let mut errors = Vec::new();
    for name in names {
        match env.functions.unset(&name.value) {
            Ok(_) => (),
            Err(error) => errors.push(UnsetFunctionsError {
                name,
                read_only_location: error.existing.read_only_location.clone().unwrap(),
            }),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Creates a message that describes the errors.
///
/// See [`arrange_message_and_divert`] for the second return value.
#[must_use]
pub fn unset_functions_error_message(
    env: &mut Env,
    errors: &[UnsetFunctionsError<'_>],
) -> (String, yash_env::semantics::Result) {
    let annotations = errors
        .iter()
        .flat_map(|error| {
            [
                Annotation::new(
                    AnnotationType::Error,
                    error.to_string().into(),
                    &error.name.origin,
                ),
                Annotation::new(
                    AnnotationType::Info,
                    format!("function `{}` was made read-only here", error.name).into(),
                    &error.read_only_location,
                ),
            ]
        })
        .collect();
    let message = Message {
        r#type: AnnotationType::Error,
        title: "cannot unset function".into(),
        annotations,
    };
    arrange_message_and_divert(env, message)
}

/// Prints an error message to the standard error.
///
/// This function constructs a message with [`unset_functions_error_message`]
/// and prints it with [`SharedSystem::print_error`].
pub async fn report_functions_error(
    env: &mut Env,
    errors: &[UnsetFunctionsError<'_>],
) -> crate::Result {
    let (message, divert) = unset_functions_error_message(env, errors);
    env.system.print_error(&message).await;
    crate::Result::with_exit_status_and_divert(ExitStatus::FAILURE, divert)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use yash_env::function::Function;
    use yash_env::variable::Value;
    use yash_syntax::source::Location;
    use yash_syntax::syntax::FullCompoundCommand;

    #[test]
    fn unsetting_one_variable() {
        let mut env = Env::new_virtual();
        env.get_or_create_variable("foo", Global)
            .assign("FOO", None)
            .unwrap();
        env.get_or_create_variable("bar", Global)
            .assign("BAR", None)
            .unwrap();
        env.get_or_create_variable("baz", Global)
            .assign("BAZ", None)
            .unwrap();

        unset_variables(&mut env, &Field::dummies(["bar"])).unwrap();
        assert_eq!(
            env.variables.get("foo").unwrap().value,
            Some(Value::scalar("FOO")),
        );
        assert_eq!(env.variables.get("bar"), None);
        assert_eq!(
            env.variables.get("baz").unwrap().value,
            Some(Value::scalar("BAZ")),
        );
    }

    #[test]
    fn unsetting_many_variables() {
        let mut env = Env::new_virtual();
        env.get_or_create_variable("foo", Global)
            .assign("FOO", None)
            .unwrap();
        env.get_or_create_variable("bar", Global)
            .assign("BAR", None)
            .unwrap();
        env.get_or_create_variable("baz", Global)
            .assign("BAZ", None)
            .unwrap();

        unset_variables(&mut env, &Field::dummies(["bar", "foo", "baz"])).unwrap();
        assert_eq!(env.variables.get("foo"), None);
        assert_eq!(env.variables.get("bar"), None);
        assert_eq!(env.variables.get("baz"), None);
    }

    #[test]
    fn unsetting_readonly_variables() {
        let mut env = Env::new_virtual();
        let mut a = env.get_or_create_variable("a", Global);
        a.assign("A", None).unwrap();
        let mut b = env.get_or_create_variable("b", Global);
        b.assign("B", None).unwrap();
        let location_b = Location::dummy("readonly b");
        b.make_read_only(location_b.clone());
        let mut c = env.get_or_create_variable("c", Global);
        c.assign("C", None).unwrap();
        let location_c = Location::dummy("readonly c");
        c.make_read_only(location_c.clone());
        let mut d = env.get_or_create_variable("d", Global);
        d.assign("D", None).unwrap();
        let names = Field::dummies(["a", "b", "c", "d"]);

        let errors = unset_variables(&mut env, &names).unwrap_err();
        assert_matches!(&errors[..], [e1, e2] => {
            assert_eq!(e1.name, &Field::dummy("b"));
            assert_eq!(e1.read_only_location, location_b);
            assert_eq!(e2.name, &Field::dummy("c"));
            assert_eq!(e2.read_only_location, location_c);
        });
        assert_eq!(env.variables.get("a"), None);
        assert_eq!(
            env.variables.get("b").unwrap().value,
            Some(Value::scalar("B")),
        );
        assert_eq!(
            env.variables.get("c").unwrap().value,
            Some(Value::scalar("C")),
        );
        assert_eq!(env.variables.get("d"), None);
    }

    fn dummy_function(name: &str) -> Function {
        Function::new(
            name,
            "{ :; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy(name),
        )
    }

    #[test]
    fn unsetting_one_function() {
        let mut env = Env::new_virtual();
        env.functions.define(dummy_function("foo")).unwrap();
        env.functions.define(dummy_function("bar")).unwrap();
        env.functions.define(dummy_function("baz")).unwrap();

        unset_functions(&mut env, &Field::dummies(["foo"])).unwrap();
        assert_eq!(env.functions.get("foo"), None);
        assert_eq!(env.functions.get("bar").unwrap().name, "bar");
        assert_eq!(env.functions.get("baz").unwrap().name, "baz");
    }

    #[test]
    fn unsetting_many_functions() {
        let mut env = Env::new_virtual();
        env.functions.define(dummy_function("foo")).unwrap();
        env.functions.define(dummy_function("bar")).unwrap();
        env.functions.define(dummy_function("baz")).unwrap();

        unset_functions(&mut env, &Field::dummies(["bar", "foo", "baz"])).unwrap();
        assert_eq!(env.functions.get("foo"), None);
        assert_eq!(env.functions.get("bar"), None);
        assert_eq!(env.functions.get("baz"), None);
    }

    #[test]
    fn unsetting_readonly_function() {
        let mut env = Env::new_virtual();
        env.functions.define(dummy_function("a")).unwrap();
        let location_b = Location::dummy("readonly b");
        env.functions
            .define(dummy_function("b").make_read_only(location_b.clone()))
            .unwrap();
        let location_c = Location::dummy("readonly c");
        env.functions
            .define(dummy_function("c").make_read_only(location_c.clone()))
            .unwrap();
        env.functions.define(dummy_function("d")).unwrap();
        let names = Field::dummies(["a", "b", "c", "d"]);

        let errors = unset_functions(&mut env, &names).unwrap_err();
        assert_matches!(&errors[..], [e1, e2] => {
            assert_eq!(e1.name, &Field::dummy("b"));
            assert_eq!(e1.read_only_location, location_b);
            assert_eq!(e2.name, &Field::dummy("c"));
            assert_eq!(e2.read_only_location, location_c);
        });
        assert_eq!(env.functions.get("a"), None);
        assert_eq!(env.functions.get("b").unwrap().name, "b");
        assert_eq!(env.functions.get("c").unwrap().name, "c");
        assert_eq!(env.functions.get("d"), None);
    }

    // TODO unsetting_global_variable_hidden_by_local_variable: should unset the both
    // TODO unsetting_readonly_global_variable_hidden_by_local_variable: should unset the local only
}
