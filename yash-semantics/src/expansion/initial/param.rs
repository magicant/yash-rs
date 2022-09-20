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

//! Parameter expansion

use super::super::phrase::Phrase;
use super::super::AttrChar;
use super::super::Error;
use super::super::Origin;
use super::Env;
use std::borrow::Cow;
use yash_env::variable::Value;
use yash_syntax::source::Location;
use yash_syntax::syntax::Modifier;
use yash_syntax::syntax::Param;

/// Reference to a parameter expansion
pub struct ParamRef<'a> {
    pub name: &'a str,
    pub modifier: &'a Modifier,
    pub location: &'a Location,
}

impl<'a> From<&'a Param> for ParamRef<'a> {
    fn from(param: &'a Param) -> Self {
        ParamRef {
            name: &param.name,
            modifier: &param.modifier,
            location: &param.location,
        }
    }
}

mod lookup;
mod switch;

impl ParamRef<'_> {
    /// Performs parameter expansion.
    pub async fn expand(&self, env: &mut Env<'_>) -> Result<Phrase, Error> {
        // TODO Expand and parse Index

        // Lookup //
        let lookup = match lookup::look_up_special_parameter(env.inner, self.name) {
            Some(lookup) => lookup,
            None => lookup::look_up_ordinary_parameter(&env.inner.variables, self.name),
        };

        // TODO Apply Index

        let mut value = lookup.into_owned();

        // Switch //
        if let Modifier::Switch(switch) = &self.modifier {
            if let Some(result) = switch::apply(env, switch, &mut value).await {
                return result;
            }
        }

        // TODO Check for nounset error
        // TODO Trim & Subst

        // Length modifier //
        if *self.modifier == Modifier::Length {
            // TODO Reject ${#*} and ${#@} in POSIX mode
            match &mut value {
                None => (),
                Some(Value::Scalar(v)) => to_length(v),
                Some(Value::Array(vs)) => vs.iter_mut().for_each(to_length),
            }
        }

        let mut phrase = into_phrase(value);
        if !env.will_split && self.name == "*" {
            phrase = Phrase::Field(phrase.ifs_join(&env.inner.variables));
        }
        Ok(phrase)
    }
}

/// Modifies a string to its length.
fn to_length(v: &mut String) {
    *v = v.chars().count().to_string()
}

/// Converts a value into a phrase.
fn into_phrase(value: Option<Value>) -> Phrase {
    match value {
        None => Phrase::one_empty_field(),
        Some(Value::Scalar(value)) => Phrase::Field(to_field(&value)),
        Some(Value::Array(values)) => {
            Phrase::Full(values.into_iter().map(|value| to_field(&value)).collect())
        }
    }
}

/// Converts a string to a `Vec<AttrChar>`.
fn to_field(value: &str) -> Vec<AttrChar> {
    value
        .chars()
        .map(|c| AttrChar {
            value: c,
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        })
        .collect()
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use futures_util::FutureExt;
    use yash_env::variable::Scope;
    use yash_env::variable::Variable;

    pub fn env_with_positional_params_and_ifs() -> yash_env::Env {
        let mut env = yash_env::Env::new_virtual();
        env.variables.positional_params_mut().value =
            Value::Array(vec!["a".to_string(), "c".to_string()]);
        env.variables
            .assign(Scope::Global, "IFS".to_string(), Variable::new("&?!"))
            .unwrap();
        env
    }

    pub fn param<N: ToString>(name: N) -> Param {
        Param {
            name: name.to_string(),
            modifier: Modifier::None,
            location: Location::dummy(""),
        }
    }

    #[test]
    fn basic_expansion() {
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .assign(
                Scope::Global,
                "foo".to_string(),
                Variable::new("a1\u{30A4}"),
            )
            .unwrap();
        let mut env = Env::new(&mut env);
        let param = param("foo");
        let param = ParamRef::from(&param);

        let phrase = param.expand(&mut env).now_or_never().unwrap().unwrap();
        assert_eq!(phrase, Phrase::Field(to_field("a1\u{30A4}")));
    }

    #[test]
    fn length_of_scalar() {
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .assign(
                Scope::Global,
                "foo".to_string(),
                Variable::new("a1\u{30A4}"),
            )
            .unwrap();
        let mut env = Env::new(&mut env);
        let mut param = param("foo");
        param.modifier = Modifier::Length;
        let param = ParamRef::from(&param);

        let phrase = param.expand(&mut env).now_or_never().unwrap().unwrap();
        assert_eq!(phrase, Phrase::Field(to_field("3")));
    }

    #[test]
    fn length_of_array() {
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .assign(
                Scope::Global,
                "foo".to_string(),
                Variable::new_array(["", "foo", "1", "bar"]),
            )
            .unwrap();
        let mut env = Env::new(&mut env);
        let mut param = param("foo");
        param.modifier = Modifier::Length;
        let param = ParamRef::from(&param);

        let phrase = param.expand(&mut env).now_or_never().unwrap().unwrap();
        assert_eq!(
            phrase,
            Phrase::Full(vec![
                to_field("0"),
                to_field("3"),
                to_field("1"),
                to_field("3")
            ])
        );
    }

    #[test]
    fn alter_empty() {
        use yash_syntax::syntax::{Switch, SwitchCondition, SwitchType};

        let mut env = env_with_positional_params_and_ifs();
        env.variables
            .assign(Scope::Global, "foo".to_string(), Variable::new(""))
            .unwrap();
        let mut param = param("foo");
        param.modifier = Modifier::Switch(Switch {
            r#type: SwitchType::Alter,
            condition: SwitchCondition::Unset,
            word: "bar".parse().unwrap(),
        });
        let param = ParamRef::from(&param);
        let mut env = Env::new(&mut env);

        let phrase = param.expand(&mut env).now_or_never().unwrap().unwrap();
        assert_eq!(phrase, Phrase::Field(to_field("bar")));
    }

    #[test]
    fn expand_at_no_join_in_non_splitting_context() {
        let mut env = env_with_positional_params_and_ifs();
        let param = param("@");
        let param = ParamRef::from(&param);
        let mut env = Env::new(&mut env);
        env.will_split = false;

        let phrase = param.expand(&mut env).now_or_never().unwrap().unwrap();
        assert_eq!(phrase, Phrase::Full(vec![to_field("a"), to_field("c")]));
    }

    #[test]
    fn expand_asterisk_no_join_in_splitting_context() {
        let mut env = env_with_positional_params_and_ifs();
        let param = param("*");
        let param = ParamRef::from(&param);
        let mut env = Env::new(&mut env);

        let phrase = param.expand(&mut env).now_or_never().unwrap().unwrap();
        assert_eq!(phrase, Phrase::Full(vec![to_field("a"), to_field("c")]));
    }

    #[test]
    fn expand_asterisk_ifs_join_in_non_splitting_context() {
        let mut env = env_with_positional_params_and_ifs();
        let param = param("*");
        let param = ParamRef::from(&param);
        let mut env = Env::new(&mut env);
        env.will_split = false;

        let phrase = param.expand(&mut env).now_or_never().unwrap().unwrap();
        assert_eq!(phrase, Phrase::Field(to_field("a&c")));
    }

    #[test]
    fn none_into_phrase() {
        assert_eq!(into_phrase(None), Phrase::one_empty_field());
    }

    #[test]
    fn scalar_into_phrase() {
        let result = into_phrase(Some(Value::Scalar("".to_string())));
        assert_eq!(result, Phrase::one_empty_field());

        let result = into_phrase(Some(Value::Scalar("foo".to_string())));
        assert_eq!(result, Phrase::Field(to_field("foo")));
    }

    #[test]
    fn array_into_phrase() {
        let result = into_phrase(Some(Value::Array(vec![])));
        assert_eq!(result, Phrase::zero_fields());

        let values = vec!["foo".to_string(), "bar".to_string()];
        let result = into_phrase(Some(Value::Array(values)));
        assert_eq!(result, Phrase::Full(vec![to_field("foo"), to_field("bar")]));
    }

    #[test]
    fn empty_to_field() {
        let result = to_field("");
        assert_eq!(result, []);
    }

    #[test]
    fn non_empty_to_field() {
        let result = to_field("bar");
        let b = AttrChar {
            value: 'b',
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        };
        let a = AttrChar { value: 'a', ..b };
        let r = AttrChar { value: 'r', ..b };
        assert_eq!(result, [b, a, r]);
    }
}
