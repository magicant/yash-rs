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

/// State of a [value](Value) that may be considered as "not set"
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ValueState {
    /// The variable is not set.
    Unset,
    /// The value is a scalar with no characters.
    EmptyScalar,
    /// The value is an array with no elements.
    ValuelessArray,
    /// The value is an array with one element containing no characters.
    EmptyValueArray,
}

impl ValueState {
    /// Computes the state of a value.
    ///
    /// Returns `None` if the value does not fall under any of the `ValueState`
    /// variants.
    #[must_use]
    pub fn of<'a, I: Into<Option<&'a Value>>>(value: I) -> Option<ValueState> {
        fn inner(value: Option<&Value>) -> Option<ValueState> {
            use ValueState::*;
            match value {
                None => Some(Unset),
                Some(Value::Scalar(scalar)) if scalar.is_empty() => Some(EmptyScalar),
                Some(Value::Array(array)) if array.is_empty() => Some(ValuelessArray),
                Some(Value::Array(array)) if array.len() == 1 && array[0].is_empty() => {
                    Some(EmptyValueArray)
                }
                Some(_) => None,
            }
        }
        inner(value.into())
    }

    pub fn description(&self) -> &'static str {
        use ValueState::*;
        match self {
            Unset => "unset variable",
            EmptyScalar => "empty string",
            ValuelessArray => "empty array",
            EmptyValueArray => "array with empty string",
        }
    }
}

impl std::fmt::Display for ValueState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.description().fmt(f)
    }
}

/// Error caused by an error switch
///
/// `UnsetError` is an error that occurs when you apply a switch with
/// `SwitchCondition::Error` to an empty value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct EmptyError {
    /// State of the variable value that caused this error
    pub state: ValueState,
}

impl std::fmt::Display for EmptyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.state.fmt(f)
    }
}

impl std::error::Error for EmptyError {}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ValueCondition {
    Set,
    Unset,
}

impl ValueCondition {
    fn with(cond: SwitchCondition, value: &Option<Value>) -> Self {
        match value {
            None => ValueCondition::Unset,
            Some(value) => match cond {
                SwitchCondition::Unset => ValueCondition::Set,
                SwitchCondition::UnsetOrEmpty => match value {
                    Value::Scalar(value) if value.is_empty() => ValueCondition::Unset,
                    Value::Array(values)
                        if values.is_empty() || values.len() == 1 && values[0].is_empty() =>
                    {
                        ValueCondition::Unset
                    }
                    _ => ValueCondition::Set,
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
    use ValueCondition::*;
    match (switch.r#type, ValueCondition::with(switch.condition, value)) {
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
    fn value_state_from_value() {
        let state = ValueState::of(&None);
        assert_eq!(state, Some(ValueState::Unset));
        let state = ValueState::of(&Some(Value::Scalar("".to_string())));
        assert_eq!(state, Some(ValueState::EmptyScalar));
        let state = ValueState::of(&Some(Value::Scalar(".".to_string())));
        assert_eq!(state, None);
        let state = ValueState::of(&Some(Value::Array(vec![])));
        assert_eq!(state, Some(ValueState::ValuelessArray));
        let state = ValueState::of(&Some(Value::Array(vec!["".to_string()])));
        assert_eq!(state, Some(ValueState::EmptyValueArray));
        let state = ValueState::of(&Some(Value::Array(vec![".".to_string()])));
        assert_eq!(state, None);
        let state = ValueState::of(&Some(Value::Array(vec!["".to_string(), "".to_string()])));
        assert_eq!(state, None);
    }

    #[test]
    fn value_condition() {
        let unset = None;
        let empty_scalar = Some(Value::Scalar("".to_string()));
        let non_empty_scalar = Some(Value::Scalar(".".to_string()));
        let valueless_array = Some(Value::Array(vec![]));
        let empty_value_array = Some(Value::Array(vec!["".to_string()]));
        let non_empty_value_array = Some(Value::Array(vec![".".to_string()]));
        let multi_value_array = Some(Value::Array(vec!["".to_string(), "".to_string()]));

        let condition = ValueCondition::with(SwitchCondition::Unset, &unset);
        assert_eq!(condition, ValueCondition::Unset);
        let condition = ValueCondition::with(SwitchCondition::Unset, &empty_scalar);
        assert_eq!(condition, ValueCondition::Set);
        let condition = ValueCondition::with(SwitchCondition::Unset, &non_empty_scalar);
        assert_eq!(condition, ValueCondition::Set);
        let condition = ValueCondition::with(SwitchCondition::Unset, &valueless_array);
        assert_eq!(condition, ValueCondition::Set);
        let condition = ValueCondition::with(SwitchCondition::Unset, &empty_value_array);
        assert_eq!(condition, ValueCondition::Set);
        let condition = ValueCondition::with(SwitchCondition::Unset, &non_empty_value_array);
        assert_eq!(condition, ValueCondition::Set);
        let condition = ValueCondition::with(SwitchCondition::Unset, &multi_value_array);
        assert_eq!(condition, ValueCondition::Set);

        let condition = ValueCondition::with(SwitchCondition::UnsetOrEmpty, &unset);
        assert_eq!(condition, ValueCondition::Unset);
        let condition = ValueCondition::with(SwitchCondition::UnsetOrEmpty, &empty_scalar);
        assert_eq!(condition, ValueCondition::Unset);
        let condition = ValueCondition::with(SwitchCondition::UnsetOrEmpty, &non_empty_scalar);
        assert_eq!(condition, ValueCondition::Set);
        let condition = ValueCondition::with(SwitchCondition::UnsetOrEmpty, &valueless_array);
        assert_eq!(condition, ValueCondition::Unset);
        let condition = ValueCondition::with(SwitchCondition::UnsetOrEmpty, &empty_value_array);
        assert_eq!(condition, ValueCondition::Unset);
        let condition = ValueCondition::with(SwitchCondition::UnsetOrEmpty, &non_empty_value_array);
        assert_eq!(condition, ValueCondition::Set);
        let condition = ValueCondition::with(SwitchCondition::UnsetOrEmpty, &multi_value_array);
        assert_eq!(condition, ValueCondition::Set);
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
