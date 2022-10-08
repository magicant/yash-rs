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

//! Resolving parameter names to values

use super::name::Name;
use std::borrow::Cow;
use yash_env::variable::Value;
use yash_env::Env;

/// Result of parameter name resolution
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Resolve<'a> {
    Unset,
    Scalar(Cow<'a, str>),
    Array(Cow<'a, [String]>),
}

impl From<String> for Resolve<'static> {
    fn from(value: String) -> Self {
        Resolve::Scalar(Cow::Owned(value))
    }
}

impl<'a> From<&'a str> for Resolve<'a> {
    fn from(value: &'a str) -> Self {
        Resolve::Scalar(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for Resolve<'a> {
    fn from(value: &'a String) -> Self {
        Resolve::Scalar(Cow::Borrowed(value))
    }
}

impl From<Option<String>> for Resolve<'static> {
    fn from(value: Option<String>) -> Self {
        match value {
            Some(value) => value.into(),
            None => Resolve::Unset,
        }
    }
}

impl From<Vec<String>> for Resolve<'static> {
    fn from(values: Vec<String>) -> Self {
        Resolve::Array(Cow::Owned(values))
    }
}

impl<'a> From<&'a [String]> for Resolve<'a> {
    fn from(values: &'a [String]) -> Self {
        Resolve::Array(Cow::Borrowed(values))
    }
}

impl<'a> From<&'a Vec<String>> for Resolve<'a> {
    fn from(values: &'a Vec<String>) -> Self {
        Resolve::Array(Cow::Borrowed(values))
    }
}

impl From<Value> for Resolve<'static> {
    fn from(value: Value) -> Self {
        match value {
            Value::Scalar(value) => Resolve::from(value),
            Value::Array(values) => Resolve::from(values),
        }
    }
}

impl<'a> From<&'a Value> for Resolve<'a> {
    fn from(value: &'a Value) -> Self {
        match value {
            Value::Scalar(value) => Resolve::from(value),
            Value::Array(values) => Resolve::from(values),
        }
    }
}

impl From<Option<Value>> for Resolve<'static> {
    fn from(value: Option<Value>) -> Self {
        match value {
            Some(value) => value.into(),
            None => Resolve::Unset,
        }
    }
}

impl<'a, V> From<Option<&'a V>> for Resolve<'a>
where
    Resolve<'a>: From<&'a V>,
{
    fn from(value: Option<&'a V>) -> Self {
        match value {
            Some(value) => value.into(),
            None => Resolve::Unset,
        }
    }
}

impl Resolve<'_> {
    /// Converts into an owned value
    pub fn into_owned(self) -> Option<Value> {
        match self {
            Resolve::Unset => None,
            Resolve::Scalar(value) => Some(Value::Scalar(value.into_owned())),
            Resolve::Array(values) => Some(Value::Array(values.into_owned())),
        }
    }

    /// Returns the "length" of the value.
    ///
    /// For `Unset`, the length is 0.
    /// For `Scalar`, the length is the number of characters.
    /// For `Array`, the length is the number of strings.
    pub fn len(&self) -> usize {
        match self {
            Resolve::Unset => 0,
            Resolve::Scalar(value) => value.len(),
            Resolve::Array(values) => values.len(),
        }
    }
}

/// Resolves a parameter name to its value.
pub fn resolve<'a>(env: &'a Env, name: Name<'_>) -> Resolve<'a> {
    fn options(env: &Env) -> Resolve {
        let mut value = String::new();
        for option in yash_env::option::Option::iter() {
            if let Some((name, state)) = option.short_name() {
                if state == env.options.get(option) {
                    value.push(name);
                }
            }
        }
        value.into()
    }
    fn positional(env: &Env) -> Resolve {
        (&env.variables.positional_params().value).into()
    }

    match name {
        Name::Variable(name) => env.variables.get(name).map(|v| &v.value).into(),
        Name::Special('@' | '*') => positional(env),
        Name::Special('#') => positional(env).len().to_string().into(),
        Name::Special('?') => env.exit_status.to_string().into(),
        Name::Special('-') => options(env),
        Name::Special('$') => env.main_pid.to_string().into(),
        Name::Special('!') => env.jobs.last_async_pid().to_string().into(),
        Name::Special('0') => env.arg0.as_str().into(),
        Name::Special(_) => Resolve::Unset,
        Name::Positional(0) => Resolve::Unset,
        Name::Positional(index) => match &env.variables.positional_params().value {
            Value::Scalar(_) => Resolve::Unset,
            Value::Array(params) => params.get(index - 1).into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::job::Pid;
    use yash_env::variable::Scope;
    use yash_env::variable::Variable;
    use yash_syntax::source::Location;

    #[test]
    fn variable_unset() {
        let env = Env::new_virtual();
        assert_eq!(resolve(&env, Name::Variable("foo")), Resolve::Unset);
        assert_eq!(resolve(&env, Name::Variable("bar")), Resolve::Unset);
        assert_eq!(resolve(&env, Name::Variable("baz")), Resolve::Unset);
    }

    #[test]
    fn variable_scalar() {
        let mut env = Env::new_virtual();
        env.variables
            .assign(Scope::Global, "x".to_string(), Variable::new("foo"))
            .unwrap();
        env.variables
            .assign(
                Scope::Global,
                "PATH".to_string(),
                Variable::new("/bin:/usr/bin")
                    .export()
                    .set_assigned_location(Location::dummy("assigned"))
                    .make_read_only(Location::dummy("read-only")),
            )
            .unwrap();

        let result = resolve(&env, Name::Variable("x"));
        assert_eq!(result, Resolve::Scalar("foo".into()));
        let result = resolve(&env, Name::Variable("PATH"));
        assert_eq!(result, Resolve::Scalar("/bin:/usr/bin".into()));
    }

    #[test]
    fn variable_array() {
        let mut env = Env::new_virtual();
        let values = ["/bin".to_string(), "/usr/bin".to_string()];
        env.variables
            .assign(Scope::Global, "x".to_string(), Variable::new_empty_array())
            .unwrap();
        env.variables
            .assign(
                Scope::Global,
                "PATH".to_string(),
                Variable::new_array(values.clone())
                    .export()
                    .set_assigned_location(Location::dummy("assigned"))
                    .make_read_only(Location::dummy("read-only")),
            )
            .unwrap();

        let result = resolve(&env, Name::Variable("x"));
        assert_eq!(result, Resolve::Array([].as_slice().into()));
        let result = resolve(&env, Name::Variable("PATH"));
        assert_eq!(result, Resolve::Array(values.as_slice().into()));
    }

    #[test]
    fn special_positional_at() {
        let mut env = Env::new_virtual();
        let result = resolve(&env, Name::Special('@'));
        assert_eq!(result, Resolve::Array([].as_slice().into()));

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables.positional_params_mut().value = Value::Array(params.clone());
        let result = resolve(&env, Name::Special('@'));
        assert_eq!(result, Resolve::Array(params.into()));
    }

    #[test]
    fn special_positional_asterisk() {
        let mut env = Env::new_virtual();
        let result = resolve(&env, Name::Special('*'));
        assert_eq!(result, Resolve::Array([].as_slice().into()));

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables.positional_params_mut().value = Value::Array(params.clone());
        let result = resolve(&env, Name::Special('*'));
        assert_eq!(result, Resolve::Array(params.into()));
    }

    #[test]
    fn special_length() {
        let mut env = Env::new_virtual();
        let result = resolve(&env, Name::Special('#'));
        assert_eq!(result, Resolve::Scalar("0".into()));

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables.positional_params_mut().value = Value::Array(params);
        let result = resolve(&env, Name::Special('#'));
        assert_eq!(result, Resolve::Scalar("3".into()));
    }

    #[test]
    fn special_exit_status() {
        let mut env = Env::new_virtual();
        let result = resolve(&env, Name::Special('?'));
        assert_eq!(result, Resolve::Scalar("0".into()));

        env.exit_status.0 = 49;
        let result = resolve(&env, Name::Special('?'));
        assert_eq!(result, Resolve::Scalar("49".into()));
    }

    #[test]
    fn special_shell_options() {
        let mut env = Env::new_virtual();
        let result = resolve(&env, Name::Special('-'));
        assert_eq!(result, Resolve::Scalar("".into()));

        use yash_env::option::{Option::*, OptionSet, State};
        env.options = OptionSet::empty();
        let result = resolve(&env, Name::Special('-'));
        assert_eq!(result, Resolve::Scalar("Cnfu".into()));

        env.options = OptionSet::default();
        env.options.set(AllExport, State::On);
        env.options.set(Verbose, State::On);
        env.options.set(Vi, State::On);
        let result = resolve(&env, Name::Special('-'));
        assert_eq!(result, Resolve::Scalar("av".into()));
    }

    #[test]
    fn special_main_pid() {
        let mut env = Env::new_virtual();
        let result = resolve(&env, Name::Special('$'));
        assert_eq!(result, Resolve::Scalar("2".into()));

        env.main_pid = Pid::from_raw(12345);
        let result = resolve(&env, Name::Special('$'));
        assert_eq!(result, Resolve::Scalar("12345".into()));
    }

    #[test]
    fn special_last_async_pid() {
        let mut env = Env::new_virtual();
        let result = resolve(&env, Name::Special('!'));
        assert_eq!(result, Resolve::Scalar("0".into()));

        env.jobs.set_last_async_pid(Pid::from_raw(72));
        let result = resolve(&env, Name::Special('!'));
        assert_eq!(result, Resolve::Scalar("72".into()));
    }

    #[test]
    fn special_arg0() {
        let mut env = Env::new_virtual();
        let result = resolve(&env, Name::Special('0'));
        assert_eq!(result, Resolve::Scalar("".into()));

        env.arg0 = "foo/bar".to_string();
        let result = resolve(&env, Name::Special('0'));
        assert_eq!(result, Resolve::Scalar("foo/bar".into()));
    }

    #[test]
    fn positional_unset() {
        let env = Env::new_virtual();
        assert_eq!(resolve(&env, Name::Positional(0)), Resolve::Unset);
        assert_eq!(resolve(&env, Name::Positional(1)), Resolve::Unset);
        assert_eq!(resolve(&env, Name::Positional(2)), Resolve::Unset);
        assert_eq!(resolve(&env, Name::Positional(10)), Resolve::Unset);
    }

    #[test]
    fn positional_set() {
        let mut env = Env::new_virtual();
        *env.variables.positional_params_mut() = Variable::new_array(["a", "b"]);

        assert_eq!(resolve(&env, Name::Positional(0)), Resolve::Unset);
        assert_eq!(resolve(&env, Name::Positional(3)), Resolve::Unset);
        assert_eq!(resolve(&env, Name::Positional(10)), Resolve::Unset);

        let result = resolve(&env, Name::Positional(1));
        assert_eq!(result, Resolve::Scalar("a".into()));
        let result = resolve(&env, Name::Positional(2));
        assert_eq!(result, Resolve::Scalar("b".into()));
    }
}
