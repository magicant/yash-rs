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
use yash_env::variable::Expansion;
use yash_env::variable::Value;
use yash_env::Env;
use yash_syntax::source::Location;

/// Resolves a parameter name to its value.
pub fn resolve<'a>(name: Name<'_>, env: &'a Env, location: &Location) -> Expansion<'a> {
    fn variable<'a>(env: &'a Env, name: &str, location: &Location) -> Expansion<'a> {
        env.variables
            .get(name)
            .map(|v| v.expand(location))
            .unwrap_or(Expansion::Unset)
    }
    fn options(env: &Env) -> Expansion {
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
    fn positional(env: &Env) -> Expansion {
        env.variables.positional_params().value.as_ref().into()
    }

    match name {
        Name::Variable(name) => variable(env, name, location),
        Name::Special('@' | '*') => positional(env),
        Name::Special('#') => positional(env).len().to_string().into(),
        Name::Special('?') => env.exit_status.to_string().into(),
        Name::Special('-') => options(env),
        Name::Special('$') => env.main_pid.to_string().into(),
        Name::Special('!') => env.jobs.last_async_pid().to_string().into(),
        Name::Special('0') => env.arg0.as_str().into(),
        Name::Special(_) => Expansion::Unset,
        Name::Positional(0) => Expansion::Unset,
        Name::Positional(index) => match &env.variables.positional_params().value {
            Some(Value::Array(params)) => params.get(index - 1).into(),
            _ => Expansion::Unset,
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
        let loc = Location::dummy("");
        assert_eq!(resolve(Name::Variable("foo"), &env, &loc), Expansion::Unset);
        assert_eq!(resolve(Name::Variable("bar"), &env, &loc), Expansion::Unset);
        assert_eq!(resolve(Name::Variable("baz"), &env, &loc), Expansion::Unset);
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
        let loc = Location::dummy("");

        let result = resolve(Name::Variable("x"), &env, &loc);
        assert_eq!(result, Expansion::Scalar("foo".into()));
        let result = resolve(Name::Variable("PATH"), &env, &loc);
        assert_eq!(result, Expansion::Scalar("/bin:/usr/bin".into()));
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
        let loc = Location::dummy("");

        let result = resolve(Name::Variable("x"), &env, &loc);
        assert_eq!(result, Expansion::Array([].as_slice().into()));
        let result = resolve(Name::Variable("PATH"), &env, &loc);
        assert_eq!(result, Expansion::Array(values.as_slice().into()));
    }

    #[test]
    fn special_positional_at() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(Name::Special('@'), &env, &loc);
        assert_eq!(result, Expansion::Array([].as_slice().into()));

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables.positional_params_mut().value = Some(Value::Array(params.clone()));
        let result = resolve(Name::Special('@'), &env, &loc);
        assert_eq!(result, Expansion::Array(params.into()));
    }

    #[test]
    fn special_positional_asterisk() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(Name::Special('*'), &env, &loc);
        assert_eq!(result, Expansion::Array([].as_slice().into()));

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables.positional_params_mut().value = Some(Value::Array(params.clone()));
        let result = resolve(Name::Special('*'), &env, &loc);
        assert_eq!(result, Expansion::Array(params.into()));
    }

    #[test]
    fn special_length() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(Name::Special('#'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("0".into()));

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables.positional_params_mut().value = Some(Value::Array(params));
        let result = resolve(Name::Special('#'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("3".into()));
    }

    #[test]
    fn special_exit_status() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(Name::Special('?'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("0".into()));

        env.exit_status.0 = 49;
        let result = resolve(Name::Special('?'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("49".into()));
    }

    #[test]
    fn special_shell_options() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(Name::Special('-'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("".into()));

        use yash_env::option::{Option::*, OptionSet, State};
        env.options = OptionSet::empty();
        let result = resolve(Name::Special('-'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("Cnfu".into()));

        env.options = OptionSet::default();
        env.options.set(AllExport, State::On);
        env.options.set(Verbose, State::On);
        env.options.set(Vi, State::On);
        let result = resolve(Name::Special('-'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("av".into()));
    }

    #[test]
    fn special_main_pid() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(Name::Special('$'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("2".into()));

        env.main_pid = Pid::from_raw(12345);
        let result = resolve(Name::Special('$'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("12345".into()));
    }

    #[test]
    fn special_last_async_pid() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(Name::Special('!'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("0".into()));

        env.jobs.set_last_async_pid(Pid::from_raw(72));
        let result = resolve(Name::Special('!'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("72".into()));
    }

    #[test]
    fn special_arg0() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(Name::Special('0'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("".into()));

        env.arg0 = "foo/bar".to_string();
        let result = resolve(Name::Special('0'), &env, &loc);
        assert_eq!(result, Expansion::Scalar("foo/bar".into()));
    }

    #[test]
    fn positional_unset() {
        let env = Env::new_virtual();
        let loc = Location::dummy("");
        assert_eq!(resolve(Name::Positional(0), &env, &loc), Expansion::Unset);
        assert_eq!(resolve(Name::Positional(1), &env, &loc), Expansion::Unset);
        assert_eq!(resolve(Name::Positional(2), &env, &loc), Expansion::Unset);
        assert_eq!(resolve(Name::Positional(10), &env, &loc), Expansion::Unset);
    }

    #[test]
    fn positional_set() {
        let mut env = Env::new_virtual();
        *env.variables.positional_params_mut() = Variable::new_array(["a", "b"]);
        let loc = Location::dummy("");

        assert_eq!(resolve(Name::Positional(0), &env, &loc), Expansion::Unset);
        assert_eq!(resolve(Name::Positional(3), &env, &loc), Expansion::Unset);
        assert_eq!(resolve(Name::Positional(10), &env, &loc), Expansion::Unset);

        let result = resolve(Name::Positional(1), &env, &loc);
        assert_eq!(result, Expansion::Scalar("a".into()));
        let result = resolve(Name::Positional(2), &env, &loc);
        assert_eq!(result, Expansion::Scalar("b".into()));
    }
}
