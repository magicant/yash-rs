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

//! Parameter lookup

use super::*;
use std::num::IntErrorKind;
use std::num::NonZeroUsize;
use yash_env::variable::VariableSet;
use yash_env::Env;

/// Result of parameter lookup
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Lookup<'a> {
    Unset,
    Scalar(Cow<'a, str>),
    Array(Cow<'a, [String]>),
}

impl From<String> for Lookup<'static> {
    fn from(value: String) -> Self {
        Lookup::Scalar(Cow::Owned(value))
    }
}

impl<'a> From<&'a str> for Lookup<'a> {
    fn from(value: &'a str) -> Self {
        Lookup::Scalar(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for Lookup<'a> {
    fn from(value: &'a String) -> Self {
        Lookup::Scalar(Cow::Borrowed(value))
    }
}

impl From<Vec<String>> for Lookup<'static> {
    fn from(values: Vec<String>) -> Self {
        Lookup::Array(Cow::Owned(values))
    }
}

impl<'a> From<&'a [String]> for Lookup<'a> {
    fn from(values: &'a [String]) -> Self {
        Lookup::Array(Cow::Borrowed(values))
    }
}

impl<'a> From<&'a Vec<String>> for Lookup<'a> {
    fn from(values: &'a Vec<String>) -> Self {
        Lookup::Array(Cow::Borrowed(values))
    }
}

impl From<Value> for Lookup<'static> {
    fn from(value: Value) -> Self {
        match value {
            Value::Scalar(value) => Lookup::from(value),
            Value::Array(values) => Lookup::from(values),
        }
    }
}

impl<'a> From<&'a Value> for Lookup<'a> {
    fn from(value: &'a Value) -> Self {
        match value {
            Value::Scalar(value) => Lookup::from(value),
            Value::Array(values) => Lookup::from(values),
        }
    }
}

impl Lookup<'_> {
    /// Converts into an owned value
    pub fn into_owned(self) -> Option<Value> {
        match self {
            Lookup::Unset => None,
            Lookup::Scalar(value) => Some(Value::Scalar(value.into_owned())),
            Lookup::Array(values) => Some(Value::Array(values.into_owned())),
        }
    }
}

/// Looks up for a special parameter.
///
/// Returns `None` if the `name` does not name a special parameter.
///
/// This function requires a mutable reference to `Env` because it needs to
/// update an internal flag if `name` is `!`.
pub fn look_up_special_parameter<'a>(env: &'a mut Env, name: &str) -> Option<Lookup<'a>> {
    let mut chars = name.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    match first {
        '@' => Some((&env.variables.positional_params().value).into()),
        '*' => todo!(),
        '#' => todo!(),
        '?' => Some(env.exit_status.to_string().into()),
        '-' => todo!(),
        '$' => Some(env.main_pid.to_string().into()),
        '!' => Some(env.jobs.expand_last_async_pid().to_string().into()),
        '0' => todo!(),
        _ => None,
    }
}

/// Looks up for a positional parameter or variable.
pub fn look_up_ordinary_parameter<'a>(vars: &'a VariableSet, name: &str) -> Lookup<'a> {
    look_up_positional_parameter(vars, name).unwrap_or_else(|| look_up_variable(vars, name))
}

/// Looks up for a positional parameter.
///
/// Returns `None` if the `name` is not a positive integer.
fn look_up_positional_parameter<'a>(vars: &'a VariableSet, name: &str) -> Option<Lookup<'a>> {
    let index = match name.parse::<NonZeroUsize>() {
        Ok(index) => index,
        Err(error) => {
            return match error.kind() {
                IntErrorKind::PosOverflow => Some(Lookup::Unset),
                _ => None,
            }
        }
    };
    let params = vars.positional_params();
    let index = index.get() - 1;
    match &params.value {
        Value::Scalar(_) => Some(Lookup::Unset),
        Value::Array(params) => match params.get(index) {
            Some(value) => Some(value.into()),
            None => Some(Lookup::Unset),
        },
    }
}

/// Looks up for a variable.
fn look_up_variable<'a>(vars: &'a VariableSet, name: &str) -> Lookup<'a> {
    match vars.get(name) {
        Some(var) => (&var.value).into(),
        None => Lookup::Unset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use yash_env::job::Pid;
    use yash_env::variable::Scope;
    use yash_env::variable::Variable;

    #[test]
    fn special_unapplicable() {
        let mut env = yash_env::Env::new_virtual();
        assert_eq!(look_up_special_parameter(&mut env, ""), None);
        assert_eq!(look_up_special_parameter(&mut env, "00"), None);
        assert_eq!(look_up_special_parameter(&mut env, "1"), None);
        assert_eq!(look_up_special_parameter(&mut env, "a"), None);
        assert_eq!(look_up_special_parameter(&mut env, ";"), None);
        assert_eq!(look_up_special_parameter(&mut env, "%"), None);
        assert_eq!(look_up_special_parameter(&mut env, "??"), None);
        assert_eq!(look_up_special_parameter(&mut env, "!?"), None);
    }

    mod special_positional_parameters {
        use super::*;

        #[test]
        fn at_in_non_splitting_context() {
            let mut env = yash_env::Env::new_virtual();
            let result = look_up_special_parameter(&mut env, "@").unwrap();
            assert_matches!(result, Lookup::Array(values)
                if values.as_ref() == [] as [String;0]);

            let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
            env.variables.positional_params_mut().value = Value::Array(params.clone());
            let result = look_up_special_parameter(&mut env, "@").unwrap();
            assert_matches!(result, Lookup::Array(values)
                if values.as_ref() == params);
        }
    }

    #[test]
    fn special_exit_status() {
        let mut env = yash_env::Env::new_virtual();
        let result = look_up_special_parameter(&mut env, "?").unwrap();
        assert_matches!(result, Lookup::Scalar(value) if value == "0");

        env.exit_status.0 = 49;
        let result = look_up_special_parameter(&mut env, "?").unwrap();
        assert_matches!(result, Lookup::Scalar(value) if value == "49");
    }

    #[test]
    fn special_main_pid() {
        let mut env = yash_env::Env::new_virtual();
        let result = look_up_special_parameter(&mut env, "$").unwrap();
        assert_matches!(result, Lookup::Scalar(value) if value == "2");
    }

    #[test]
    fn special_last_async_pid() {
        let mut env = yash_env::Env::new_virtual();
        let result = look_up_special_parameter(&mut env, "!").unwrap();
        assert_matches!(result, Lookup::Scalar(value) if value == "0");

        env.jobs.set_last_async_pid(Pid::from_raw(72));
        let result = look_up_special_parameter(&mut env, "!").unwrap();
        assert_matches!(result, Lookup::Scalar(value) if value == "72");
    }

    #[test]
    fn positional_unapplicable() {
        let vars = VariableSet::new();
        assert_eq!(look_up_positional_parameter(&vars, ""), None);
        assert_eq!(look_up_positional_parameter(&vars, "0"), None);
        assert_eq!(look_up_positional_parameter(&vars, "a"), None);
        assert_eq!(look_up_positional_parameter(&vars, "-1"), None);
        assert_eq!(look_up_positional_parameter(&vars, "1x"), None);
        assert_eq!(
            look_up_positional_parameter(&vars, "9999999999999999999999999999999999999999"),
            Some(Lookup::Unset)
        );
    }

    #[test]
    fn positional_unset() {
        let vars = VariableSet::new();
        let unset = Some(Lookup::Unset);
        assert_eq!(look_up_positional_parameter(&vars, "1"), unset);
        assert_eq!(look_up_positional_parameter(&vars, "2"), unset);
        assert_eq!(look_up_positional_parameter(&vars, "10"), unset);
    }

    #[test]
    fn positional_found() {
        let mut vars = VariableSet::new();
        *vars.positional_params_mut() = Variable {
            value: Value::Array(vec!["a".to_string(), "b".to_string()]),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: None,
        };
        let result = look_up_positional_parameter(&vars, "1").unwrap();
        assert_matches!(result, Lookup::Scalar(value) if value == "a");
        let result = look_up_positional_parameter(&vars, "2").unwrap();
        assert_matches!(result, Lookup::Scalar(value) if value == "b");
    }

    #[test]
    fn variable_unset() {
        let vars = VariableSet::new();
        assert_eq!(look_up_variable(&vars, ""), Lookup::Unset);
        assert_eq!(look_up_variable(&vars, "x"), Lookup::Unset);
        assert_eq!(look_up_variable(&vars, "PATH"), Lookup::Unset);
    }

    #[test]
    fn variable_scalar_found() {
        let mut vars = VariableSet::new();
        vars.assign(
            Scope::Global,
            "x".to_string(),
            Variable {
                value: Value::Scalar("foo".to_string()),
                last_assigned_location: None,
                is_exported: false,
                read_only_location: None,
            },
        )
        .unwrap();
        vars.assign(
            Scope::Global,
            "PATH".to_string(),
            Variable {
                value: Value::Scalar("/bin:/usr/bin".to_string()),
                last_assigned_location: Some(Location::dummy("assigned")),
                is_exported: true,
                read_only_location: Some(Location::dummy("read-only")),
            },
        )
        .unwrap();

        let result = look_up_variable(&vars, "x");
        assert_matches!(result, Lookup::Scalar(value) if value == "foo");
        let result = look_up_variable(&vars, "PATH");
        assert_matches!(result, Lookup::Scalar(value) if value == "/bin:/usr/bin");
    }

    #[test]
    fn variable_array_found() {
        let mut vars = VariableSet::new();
        vars.assign(
            Scope::Global,
            "x".to_string(),
            Variable {
                value: Value::Array(Vec::new()),
                last_assigned_location: None,
                is_exported: false,
                read_only_location: None,
            },
        )
        .unwrap();
        vars.assign(
            Scope::Global,
            "PATH".to_string(),
            Variable {
                value: Value::Array(vec!["/bin".to_string(), "/usr/bin".to_string()]),
                last_assigned_location: Some(Location::dummy("assigned")),
                is_exported: true,
                read_only_location: Some(Location::dummy("read-only")),
            },
        )
        .unwrap();

        let result = look_up_variable(&vars, "x");
        assert_matches!(result, Lookup::Array(values)
            if values.as_ref() == [] as [String;0]);
        let result = look_up_variable(&vars, "PATH");
        assert_matches!(result, Lookup::Array(values)
            if values.as_ref() == ["/bin".to_string(), "/usr/bin".to_string()]);
    }
}
