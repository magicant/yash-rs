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

//! Parameter expansion switch semantics

use super::Env;
use super::Error;
use super::Phrase;
use crate::expansion::attr::Origin;
use crate::expansion::initial::expand;
use yash_env::variable::Value;
use yash_syntax::syntax::Switch;
use yash_syntax::syntax::SwitchCondition;
use yash_syntax::syntax::SwitchType;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ValueState {
    Set,
    Unset,
}

impl ValueState {
    fn with(cond: SwitchCondition, value: &Option<Value>) -> Self {
        match value {
            None => ValueState::Unset,
            Some(value) => match cond {
                SwitchCondition::Unset => ValueState::Set,
                SwitchCondition::UnsetOrEmpty => match value {
                    Value::Scalar(value) if value.is_empty() => ValueState::Unset,
                    Value::Array(values)
                        if values.is_empty() || values.len() == 1 && values[0].is_empty() =>
                    {
                        ValueState::Unset
                    }
                    _ => ValueState::Set,
                },
            },
        }
    }
}

/// Modifies the origin of characters in the phrase to `SoftExpansion`.
///
/// This function updates the result of [`expand`]. Since the switch modifier is
/// part of a parameter expansion, the substitution produced by the switch
/// should be regarded as originating from a parameter expansion.
fn attribute(mut phrase: Phrase) -> Phrase {
    phrase.for_each_char_mut(|c| match c.origin {
        Origin::Literal => c.origin = Origin::SoftExpansion,
        Origin::HardExpansion | Origin::SoftExpansion => (),
    });
    phrase
}

/// Applies a switch.
///
/// If this function returns `Some(_)`, that should be the result of the whole
/// parameter expansion containing the switch. Otherwise, the parameter
/// expansion should continue processing other modifiers.
pub async fn apply(
    env: &mut Env<'_>,
    switch: &Switch,
    value: &mut Option<Value>,
) -> Option<Result<Phrase, Error>> {
    use SwitchType::*;
    use ValueState::*;
    match (switch.r#type, ValueState::with(switch.condition, value)) {
        (Alter, Set) | (Default, Unset) => Some(expand(env, &switch.word).await.map(attribute)),
        (Alter, Unset) | (Default, Set) => None,
        (Assign, Set) => todo!(),
        (Assign, Unset) => todo!(),
        (Error, Set) => todo!(),
        (Error, Unset) => todo!(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::to_field;
    use super::*;
    use crate::expansion::attr::AttrChar;
    use futures_util::FutureExt;
    use yash_env::variable::Value::*;
    use yash_syntax::syntax::SwitchCondition::*;
    use yash_syntax::syntax::SwitchType::*;

    #[test]
    fn value_state() {
        let unset = None;
        let empty_scalar = Some(Value::Scalar("".to_string()));
        let non_empty_scalar = Some(Value::Scalar(".".to_string()));
        let valueless_array = Some(Value::Array(vec![]));
        let empty_value_array = Some(Value::Array(vec!["".to_string()]));
        let non_empty_value_array = Some(Value::Array(vec![".".to_string()]));
        let multi_value_array = Some(Value::Array(vec!["".to_string(), "".to_string()]));

        let state = ValueState::with(SwitchCondition::Unset, &unset);
        assert_eq!(state, ValueState::Unset);
        let state = ValueState::with(SwitchCondition::Unset, &empty_scalar);
        assert_eq!(state, ValueState::Set);
        let state = ValueState::with(SwitchCondition::Unset, &non_empty_scalar);
        assert_eq!(state, ValueState::Set);
        let state = ValueState::with(SwitchCondition::Unset, &valueless_array);
        assert_eq!(state, ValueState::Set);
        let state = ValueState::with(SwitchCondition::Unset, &empty_value_array);
        assert_eq!(state, ValueState::Set);
        let state = ValueState::with(SwitchCondition::Unset, &non_empty_value_array);
        assert_eq!(state, ValueState::Set);
        let state = ValueState::with(SwitchCondition::Unset, &multi_value_array);
        assert_eq!(state, ValueState::Set);

        let state = ValueState::with(SwitchCondition::UnsetOrEmpty, &unset);
        assert_eq!(state, ValueState::Unset);
        let state = ValueState::with(SwitchCondition::UnsetOrEmpty, &empty_scalar);
        assert_eq!(state, ValueState::Unset);
        let state = ValueState::with(SwitchCondition::UnsetOrEmpty, &non_empty_scalar);
        assert_eq!(state, ValueState::Set);
        let state = ValueState::with(SwitchCondition::UnsetOrEmpty, &valueless_array);
        assert_eq!(state, ValueState::Unset);
        let state = ValueState::with(SwitchCondition::UnsetOrEmpty, &empty_value_array);
        assert_eq!(state, ValueState::Unset);
        let state = ValueState::with(SwitchCondition::UnsetOrEmpty, &non_empty_value_array);
        assert_eq!(state, ValueState::Set);
        let state = ValueState::with(SwitchCondition::UnsetOrEmpty, &multi_value_array);
        assert_eq!(state, ValueState::Set);
    }

    #[test]
    fn attributing() {
        let phrase = Phrase::Field(vec![
            AttrChar {
                value: 'a',
                origin: Origin::Literal,
                is_quoted: false,
                is_quoting: false,
            },
            AttrChar {
                value: 'b',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            },
            AttrChar {
                value: 'c',
                origin: Origin::HardExpansion,
                is_quoted: false,
                is_quoting: false,
            },
        ]);

        let phrase = attribute(phrase);
        assert_eq!(
            phrase,
            Phrase::Field(vec![
                AttrChar {
                    value: 'a',
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false,
                },
                AttrChar {
                    value: 'b',
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false,
                },
                AttrChar {
                    value: 'c',
                    origin: Origin::HardExpansion,
                    is_quoted: false,
                    is_quoting: false,
                },
            ])
        );
    }

    #[test]
    fn alter_unset() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Alter,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let mut value = None;
        let result = apply(&mut env, &switch, &mut value).now_or_never().unwrap();
        assert_eq!(result, None);
        assert_eq!(value, None);
    }

    #[test]
    fn alter_non_empty() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Alter,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let mut value = Some(Scalar("bar".to_string()));
        let result = apply(&mut env, &switch, &mut value).now_or_never().unwrap();
        assert_eq!(result, Some(Ok(Phrase::Field(to_field("foo")))));
    }

    #[test]
    fn default_unset() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Default,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let mut value = None;
        let result = apply(&mut env, &switch, &mut value).now_or_never().unwrap();
        assert_eq!(result, Some(Ok(Phrase::Field(to_field("foo")))));
    }

    #[test]
    fn default_non_empty() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Default,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let mut value = Some(Scalar("bar".to_string()));
        let result = apply(&mut env, &switch, &mut value).now_or_never().unwrap();
        assert_eq!(result, None);
        assert_eq!(value, Some(Scalar("bar".to_string())));
    }
}
