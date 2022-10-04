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

pub use super::resolve::Resolve;

/// Looks up for a special parameter.
///
/// Returns `None` if the `name` does not name a special parameter.
pub fn look_up_special_parameter<'a>(env: &'a Env, name: &str) -> Option<Resolve<'a>> {
    let mut chars = name.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    match first {
        '@' | '*' => Some((&env.variables.positional_params().value).into()),
        '#' => {
            let value = Resolve::from(&env.variables.positional_params().value);
            Some(value.len().to_string().into())
        }
        '?' => Some(env.exit_status.to_string().into()),
        '-' => {
            let mut value = String::new();
            for option in yash_env::option::Option::iter() {
                if let Some((name, state)) = option.short_name() {
                    if state == env.options.get(option) {
                        value.push(name);
                    }
                }
            }
            Some(value.into())
        }
        '$' => Some(env.main_pid.to_string().into()),
        '!' => Some(env.jobs.last_async_pid().to_string().into()),
        '0' => Some(env.arg0.as_str().into()),
        _ => None,
    }
}

/// Looks up for a positional parameter or variable.
pub fn look_up_ordinary_parameter<'a>(vars: &'a VariableSet, name: &str) -> Resolve<'a> {
    look_up_positional_parameter(vars, name).unwrap_or_else(|| look_up_variable(vars, name))
}

/// Looks up for a positional parameter.
///
/// Returns `None` if the `name` is not a positive integer.
fn look_up_positional_parameter<'a>(vars: &'a VariableSet, name: &str) -> Option<Resolve<'a>> {
    let index = match name.parse::<NonZeroUsize>() {
        Ok(index) => index,
        Err(error) => {
            return match error.kind() {
                IntErrorKind::PosOverflow => Some(Resolve::Unset),
                _ => None,
            }
        }
    };
    let params = vars.positional_params();
    let index = index.get() - 1;
    match &params.value {
        Value::Scalar(_) => Some(Resolve::Unset),
        Value::Array(params) => match params.get(index) {
            Some(value) => Some(value.into()),
            None => Some(Resolve::Unset),
        },
    }
}

/// Looks up for a variable.
fn look_up_variable<'a>(vars: &'a VariableSet, name: &str) -> Resolve<'a> {
    match vars.get(name) {
        Some(var) => (&var.value).into(),
        None => Resolve::Unset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use yash_env::job::Pid;
    use yash_env::option::OptionSet;
    use yash_env::variable::Scope;
    use yash_env::variable::Variable;

    #[test]
    fn special_unapplicable() {
        let env = yash_env::Env::new_virtual();
        assert_eq!(look_up_special_parameter(&env, ""), None);
        assert_eq!(look_up_special_parameter(&env, "00"), None);
        assert_eq!(look_up_special_parameter(&env, "1"), None);
        assert_eq!(look_up_special_parameter(&env, "a"), None);
        assert_eq!(look_up_special_parameter(&env, ";"), None);
        assert_eq!(look_up_special_parameter(&env, "%"), None);
        assert_eq!(look_up_special_parameter(&env, "??"), None);
        assert_eq!(look_up_special_parameter(&env, "!?"), None);
    }

    #[test]
    fn special_positional_parameters_at() {
        let mut env = yash_env::Env::new_virtual();
        let result = look_up_special_parameter(&env, "@").unwrap();
        assert_matches!(result, Resolve::Array(values)
            if values.as_ref() == [] as [String;0]);

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables.positional_params_mut().value = Value::Array(params.clone());
        let result = look_up_special_parameter(&env, "@").unwrap();
        assert_matches!(result, Resolve::Array(values)
            if values.as_ref() == params);
    }

    #[test]
    fn special_positional_parameters_asterisk() {
        let mut env = yash_env::Env::new_virtual();
        let result = look_up_special_parameter(&env, "*").unwrap();
        assert_matches!(result, Resolve::Array(values)
            if values.as_ref() == [] as [String;0]);

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables.positional_params_mut().value = Value::Array(params.clone());
        let result = look_up_special_parameter(&env, "*").unwrap();
        assert_matches!(result, Resolve::Array(values)
            if values.as_ref() == params);
    }

    #[test]
    fn special_length() {
        let mut env = yash_env::Env::new_virtual();
        let result = look_up_special_parameter(&env, "#").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "0");

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables.positional_params_mut().value = Value::Array(params);
        let result = look_up_special_parameter(&env, "#").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "3");
    }

    #[test]
    fn special_exit_status() {
        let mut env = yash_env::Env::new_virtual();
        let result = look_up_special_parameter(&env, "?").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "0");

        env.exit_status.0 = 49;
        let result = look_up_special_parameter(&env, "?").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "49");
    }

    #[test]
    fn special_shell_options() {
        let mut env = yash_env::Env::new_virtual();
        let result = look_up_special_parameter(&env, "-").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "");

        env.options = OptionSet::empty();
        let result = look_up_special_parameter(&env, "-").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "Cnfu");

        use yash_env::option::{Option::*, State};
        env.options = OptionSet::default();
        env.options.set(AllExport, State::On);
        env.options.set(Verbose, State::On);
        env.options.set(Vi, State::On);
        let result = look_up_special_parameter(&env, "-").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "av");
    }

    #[test]
    fn special_main_pid() {
        let env = yash_env::Env::new_virtual();
        let result = look_up_special_parameter(&env, "$").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "2");
    }

    #[test]
    fn special_last_async_pid() {
        let mut env = yash_env::Env::new_virtual();
        let result = look_up_special_parameter(&env, "!").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "0");

        env.jobs.set_last_async_pid(Pid::from_raw(72));
        let result = look_up_special_parameter(&env, "!").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "72");
    }

    #[test]
    fn special_arg0() {
        let mut env = yash_env::Env::new_virtual();
        let result = look_up_special_parameter(&env, "0").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "");

        env.arg0 = "foo/bar".to_string();
        let result = look_up_special_parameter(&env, "0").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "foo/bar");
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
            Some(Resolve::Unset)
        );
    }

    #[test]
    fn positional_unset() {
        let vars = VariableSet::new();
        let unset = Some(Resolve::Unset);
        assert_eq!(look_up_positional_parameter(&vars, "1"), unset);
        assert_eq!(look_up_positional_parameter(&vars, "2"), unset);
        assert_eq!(look_up_positional_parameter(&vars, "10"), unset);
    }

    #[test]
    fn positional_found() {
        let mut vars = VariableSet::new();
        *vars.positional_params_mut() = Variable::new_array(["a", "b"]);
        let result = look_up_positional_parameter(&vars, "1").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "a");
        let result = look_up_positional_parameter(&vars, "2").unwrap();
        assert_matches!(result, Resolve::Scalar(value) if value == "b");
    }

    #[test]
    fn variable_unset() {
        let vars = VariableSet::new();
        assert_eq!(look_up_variable(&vars, ""), Resolve::Unset);
        assert_eq!(look_up_variable(&vars, "x"), Resolve::Unset);
        assert_eq!(look_up_variable(&vars, "PATH"), Resolve::Unset);
    }

    #[test]
    fn variable_scalar_found() {
        let mut vars = VariableSet::new();
        vars.assign(Scope::Global, "x".to_string(), Variable::new("foo"))
            .unwrap();
        vars.assign(
            Scope::Global,
            "PATH".to_string(),
            Variable::new("/bin:/usr/bin")
                .export()
                .set_assigned_location(Location::dummy("assigned"))
                .make_read_only(Location::dummy("read-only")),
        )
        .unwrap();

        let result = look_up_variable(&vars, "x");
        assert_matches!(result, Resolve::Scalar(value) if value == "foo");
        let result = look_up_variable(&vars, "PATH");
        assert_matches!(result, Resolve::Scalar(value) if value == "/bin:/usr/bin");
    }

    #[test]
    fn variable_array_found() {
        let mut vars = VariableSet::new();
        vars.assign(Scope::Global, "x".to_string(), Variable::new_empty_array())
            .unwrap();
        vars.assign(
            Scope::Global,
            "PATH".to_string(),
            Variable::new_array(["/bin", "/usr/bin"])
                .export()
                .set_assigned_location(Location::dummy("assigned"))
                .make_read_only(Location::dummy("read-only")),
        )
        .unwrap();

        let result = look_up_variable(&vars, "x");
        assert_matches!(result, Resolve::Array(values)
            if values.as_ref() == [] as [String;0]);
        let result = look_up_variable(&vars, "PATH");
        assert_matches!(result, Resolve::Array(values)
            if values.as_ref() == ["/bin".to_string(), "/usr/bin".to_string()]);
    }
}
