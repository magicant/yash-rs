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

use yash_env::variable::Expansion;
use yash_env::Env;
use yash_syntax::source::Location;
use yash_syntax::syntax::Param;
use yash_syntax::syntax::ParamType::*;
use yash_syntax::syntax::SpecialParam::*;

/// Resolves a parameter name to its value.
pub fn resolve<'a>(env: &'a Env, param: &Param, location: &Location) -> Expansion<'a> {
    fn variable<'a>(env: &'a Env, name: &str, location: &Location) -> Expansion<'a> {
        env.variables
            .get(name)
            .map_or(Expansion::Unset, |v| v.expand(location))
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
    fn positional(env: &Env) -> &[String] {
        &env.variables.positional_params().values
    }

    match param.r#type {
        Variable => variable(env, &param.id, location),
        Special(At | Asterisk) => positional(env).into(),
        Special(Number) => positional(env).len().to_string().into(),
        Special(Question) => env.exit_status.to_string().into(),
        Special(Hyphen) => options(env),
        Special(Dollar) => env.main_pid.to_string().into(),
        Special(Exclamation) => env.jobs.last_async_pid().to_string().into(),
        Special(Zero) => env.arg0.as_str().into(),
        Positional(0) => Expansion::Unset,
        Positional(index) => positional(env).get(index - 1).into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::job::Pid;
    use yash_env::variable::Scope;
    use yash_env::variable::Value;
    use yash_env::variable::PATH;
    use yash_syntax::source::Location;
    use yash_syntax::syntax::SpecialParam;

    #[test]
    fn variable_unset() {
        let env = Env::new_virtual();
        let loc = Location::dummy("");
        assert_eq!(
            resolve(&env, &Param::variable("foo"), &loc),
            Expansion::Unset
        );
        assert_eq!(
            resolve(&env, &Param::variable("bar"), &loc),
            Expansion::Unset
        );
        assert_eq!(
            resolve(&env, &Param::variable("baz"), &loc),
            Expansion::Unset
        );
    }

    #[test]
    fn variable_scalar() {
        let mut env = Env::new_virtual();
        let mut x = env.variables.get_or_new("x", Scope::Global);
        x.assign("foo", None).unwrap();
        let mut path = env.variables.get_or_new(PATH, Scope::Global);
        path.assign("/bin:/usr/bin", Location::dummy("assigned"))
            .unwrap();
        path.export(true);
        path.make_read_only(Location::dummy("read-only"));
        let loc = Location::dummy("");

        let result = resolve(&env, &Param::variable("x"), &loc);
        assert_eq!(result, Expansion::Scalar("foo".into()));
        let result = resolve(&env, &Param::variable(PATH), &loc);
        assert_eq!(result, Expansion::Scalar("/bin:/usr/bin".into()));
    }

    #[test]
    fn variable_array() {
        let mut env = Env::new_virtual();
        let mut x = env.variables.get_or_new("x", Scope::Global);
        x.assign(Value::Array(vec![]), None).unwrap();
        let mut path = env.variables.get_or_new(PATH, Scope::Global);
        let values = ["/bin".to_string(), "/usr/bin".to_string()];
        path.assign(
            Value::array(values.clone()),
            Some(Location::dummy("assigned")),
        )
        .unwrap();
        path.export(true);
        path.make_read_only(Location::dummy("read-only"));
        let loc = Location::dummy("");

        let result = resolve(&env, &Param::variable("x"), &loc);
        assert_eq!(result, Expansion::Array([].as_slice().into()));
        let result = resolve(&env, &Param::variable(PATH), &loc);
        assert_eq!(result, Expansion::Array(values.as_slice().into()));
    }

    #[test]
    fn special_positional_at() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(&env, &Param::from(SpecialParam::At), &loc);
        assert_eq!(result, Expansion::Array([].as_slice().into()));

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables
            .positional_params_mut()
            .values
            .clone_from(&params);
        let result = resolve(&env, &Param::from(SpecialParam::At), &loc);
        assert_eq!(result, Expansion::Array(params.into()));
    }

    #[test]
    fn special_positional_asterisk() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(&env, &Param::from(SpecialParam::Asterisk), &loc);
        assert_eq!(result, Expansion::Array([].as_slice().into()));

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables
            .positional_params_mut()
            .values
            .clone_from(&params);
        let result = resolve(&env, &Param::from(SpecialParam::Asterisk), &loc);
        assert_eq!(result, Expansion::Array(params.into()));
    }

    #[test]
    fn special_length() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(&env, &Param::from(SpecialParam::Number), &loc);
        assert_eq!(result, Expansion::Scalar("0".into()));

        let params = vec!["a".to_string(), "foo bar".to_string(), "9".to_string()];
        env.variables.positional_params_mut().values = params;
        let result = resolve(&env, &Param::from(SpecialParam::Number), &loc);
        assert_eq!(result, Expansion::Scalar("3".into()));
    }

    #[test]
    fn special_exit_status() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(&env, &Param::from(SpecialParam::Question), &loc);
        assert_eq!(result, Expansion::Scalar("0".into()));

        env.exit_status.0 = 49;
        let result = resolve(&env, &Param::from(SpecialParam::Question), &loc);
        assert_eq!(result, Expansion::Scalar("49".into()));
    }

    #[test]
    fn special_shell_options() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(&env, &Param::from(SpecialParam::Hyphen), &loc);
        assert_eq!(result, Expansion::Scalar("".into()));

        use yash_env::option::{Option::*, OptionSet, State};
        env.options = OptionSet::empty();
        let result = resolve(&env, &Param::from(SpecialParam::Hyphen), &loc);
        assert_eq!(result, Expansion::Scalar("Cnfu".into()));

        env.options = OptionSet::default();
        env.options.set(AllExport, State::On);
        env.options.set(Verbose, State::On);
        env.options.set(Vi, State::On);
        let result = resolve(&env, &Param::from(SpecialParam::Hyphen), &loc);
        assert_eq!(result, Expansion::Scalar("av".into()));
    }

    #[test]
    fn special_main_pid() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(&env, &Param::from(SpecialParam::Dollar), &loc);
        assert_eq!(result, Expansion::Scalar("2".into()));

        env.main_pid = Pid(12345);
        let result = resolve(&env, &Param::from(SpecialParam::Dollar), &loc);
        assert_eq!(result, Expansion::Scalar("12345".into()));
    }

    #[test]
    fn special_last_async_pid() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(&env, &Param::from(SpecialParam::Exclamation), &loc);
        assert_eq!(result, Expansion::Scalar("0".into()));

        env.jobs.set_last_async_pid(Pid(72));
        let result = resolve(&env, &Param::from(SpecialParam::Exclamation), &loc);
        assert_eq!(result, Expansion::Scalar("72".into()));
    }

    #[test]
    fn special_arg0() {
        let mut env = Env::new_virtual();
        let loc = Location::dummy("");
        let result = resolve(&env, &Param::from(SpecialParam::Zero), &loc);
        assert_eq!(result, Expansion::Scalar("".into()));

        env.arg0 = "foo/bar".to_string();
        let result = resolve(&env, &Param::from(SpecialParam::Zero), &loc);
        assert_eq!(result, Expansion::Scalar("foo/bar".into()));
    }

    #[test]
    fn positional_unset() {
        let env = Env::new_virtual();
        let loc = Location::dummy("");
        assert_eq!(resolve(&env, &Param::from(0), &loc), Expansion::Unset);
        assert_eq!(resolve(&env, &Param::from(1), &loc), Expansion::Unset);
        assert_eq!(resolve(&env, &Param::from(2), &loc), Expansion::Unset);
        assert_eq!(resolve(&env, &Param::from(10), &loc), Expansion::Unset);
    }

    #[test]
    fn positional_set() {
        let mut env = Env::new_virtual();
        env.variables.positional_params_mut().values = vec!["a".to_string(), "b".to_string()];
        let loc = Location::dummy("");

        assert_eq!(resolve(&env, &Param::from(0), &loc), Expansion::Unset);
        assert_eq!(resolve(&env, &Param::from(3), &loc), Expansion::Unset);
        assert_eq!(resolve(&env, &Param::from(10), &loc), Expansion::Unset);

        let result = resolve(&env, &Param::from(1), &loc);
        assert_eq!(result, Expansion::Scalar("a".into()));
        let result = resolve(&env, &Param::from(2), &loc);
        assert_eq!(result, Expansion::Scalar("b".into()));
    }
}
