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

use super::name::Name;
use super::Env;
use super::Error;
use super::Phrase;
use crate::expansion::attr::Origin;
use crate::expansion::attr_strip::Strip;
use crate::expansion::expand_word;
use crate::expansion::initial::expand;
use crate::expansion::quote_removal::skip_quotes;
use crate::expansion::ErrorCause;
use yash_env::variable::Scope;
use yash_env::variable::Value;
use yash_syntax::source::Location;
use yash_syntax::syntax::Switch;
use yash_syntax::syntax::SwitchCondition;
use yash_syntax::syntax::SwitchType;
use yash_syntax::syntax::Word;

/// Physical state of a [value](Value) that may be considered as "not set"
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
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
#[error("{} ({})", self.message_or_default(), .state.description())]
#[non_exhaustive]
pub struct EmptyError {
    /// State of the variable value that caused this error
    pub state: ValueState,
    /// Error message specified in the switch
    pub message: Option<String>,
}

impl EmptyError {
    /// Returns the message.
    ///
    /// If `self.message` is `Some(_)`, its content is returned. Otherwise, the
    /// default message is returned.
    #[must_use]
    pub fn message_or_default(&self) -> &str {
        self.message
            .as_deref()
            .unwrap_or("parameter expansion with empty value")
    }
}

/// Error caused by an assign switch
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
#[non_exhaustive]
pub enum NonassignableError {
    /// The parameter is not a variable.
    #[error("not an assignable variable")]
    NotVariable,
    // /// The parameter expansion refers to an array but does not index a single
    // /// element.
    // #[error("cannot assign to a non-scalar array range")]
    // TODO ArrayIndex,

    // /// The parameter expansion is nested.
    // #[error("cannot assign to a nested parameter expansion")]
    // TODO Nested,
}

/// Abstract state of a [value](Value) that determines the effect of a switch
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ValueCondition {
    Set,
    Unset(ValueState),
}

impl ValueCondition {
    fn with<S: Into<Option<ValueState>>>(cond: SwitchCondition, state: S) -> Self {
        fn inner(cond: SwitchCondition, state: Option<ValueState>) -> ValueCondition {
            match (cond, state) {
                (_, None) => ValueCondition::Set,
                (SwitchCondition::UnsetOrEmpty, Some(state)) => ValueCondition::Unset(state),
                (_, Some(ValueState::Unset)) => ValueCondition::Unset(ValueState::Unset),
                (SwitchCondition::Unset, Some(ValueState::EmptyScalar)) => ValueCondition::Set,
                (SwitchCondition::Unset, Some(ValueState::ValuelessArray)) => ValueCondition::Set,
                (SwitchCondition::Unset, Some(ValueState::EmptyValueArray)) => ValueCondition::Set,
            }
        }
        inner(cond, state.into())
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

/// Assigns the expansion of `value` to variable `name`.
async fn assign(
    env: &mut Env<'_>,
    name: Option<Name<'_>>,
    value: &Word,
    location: Location,
) -> Result<Phrase, Error> {
    // TODO Support assignment to an array element
    let name = match name {
        Some(Name::Variable(name)) => name.to_owned(),
        _ => {
            let cause = ErrorCause::NonassignableParameter(NonassignableError::NotVariable);
            return Err(Error { cause, location });
        }
    };
    let value_phrase = attribute(expand(env, value).await?);
    let joined_value = value_phrase.clone().ifs_join(&env.inner.variables);
    let final_value = skip_quotes(joined_value).strip().collect::<String>();
    env.inner
        .get_or_create_variable(name, Scope::Global)
        .assign(final_value.into(), Some(location))
        .map_err(|e| {
            let location = e.assigned_location.as_ref().unwrap().clone();
            let cause = ErrorCause::AssignError(e);
            Error { cause, location }
        })?;
    Ok(value_phrase)
}

/// Expands a word to be used as an empty expansion error message.
async fn empty_expansion_error_message(
    env: &mut Env<'_>,
    message_word: &Word,
) -> Result<Option<String>, Error> {
    if message_word.units.is_empty() {
        return Ok(None);
    }

    let (message_field, exit_status) = expand_word(env.inner, message_word).await?;
    if exit_status.is_some() {
        env.last_command_subst_exit_status = exit_status;
    }
    Ok(Some(message_field.value))
}

/// Constructs an empty expansion error.
async fn empty_expansion_error(
    env: &mut Env<'_>,
    state: ValueState,
    message_word: &Word,
    location: Location,
) -> Error {
    let message = match empty_expansion_error_message(env, message_word).await {
        Ok(message) => message,
        Err(error) => return error,
    };
    let cause = ErrorCause::EmptyExpansion(EmptyError { state, message });
    Error { cause, location }
}

/// Applies a switch.
///
/// If this function returns `Some(_)`, that should be the result of the whole
/// parameter expansion containing the switch. Otherwise, the parameter
/// expansion should continue processing other modifiers.
pub async fn apply(
    env: &mut Env<'_>,
    switch: &Switch,
    name: Option<Name<'_>>,
    value: &mut Option<Value>,
    location: &Location,
) -> Option<Result<Phrase, Error>> {
    use SwitchType::*;
    use ValueCondition::*;
    let cond = ValueCondition::with(switch.condition, ValueState::of(&*value));
    match (switch.r#type, cond) {
        (Alter, Unset(_)) | (Default, Set) | (Assign, Set) | (Error, Set) => None,
        (Alter, Set) | (Default, Unset(_)) => Some(expand(env, &switch.word).await.map(attribute)),
        (Assign, Unset(_)) => Some(assign(env, name, &switch.word, location.clone()).await),
        (Error, Unset(state)) => Some(Err(empty_expansion_error(
            env,
            state,
            &switch.word,
            location.clone(),
        )
        .await)),
    }
}

#[cfg(test)]
mod tests {
    use super::super::to_field;
    use super::*;
    use crate::expansion::attr::AttrChar;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use yash_env::variable::Value::*;
    use yash_syntax::syntax::SwitchCondition::*;
    use yash_syntax::syntax::SwitchType::*;

    #[test]
    fn value_state_from_value() {
        let state = ValueState::of(&None);
        assert_eq!(state, Some(ValueState::Unset));
        let state = ValueState::of(&Some(Value::scalar("")));
        assert_eq!(state, Some(ValueState::EmptyScalar));
        let state = ValueState::of(&Some(Value::scalar(".")));
        assert_eq!(state, None);
        let state = ValueState::of(&Some(Value::Array(vec![])));
        assert_eq!(state, Some(ValueState::ValuelessArray));
        let state = ValueState::of(&Some(Value::array([""])));
        assert_eq!(state, Some(ValueState::EmptyValueArray));
        let state = ValueState::of(&Some(Value::array(["."])));
        assert_eq!(state, None);
        let state = ValueState::of(&Some(Value::array(["", ""])));
        assert_eq!(state, None);
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
    fn alter_with_unset_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Alter,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = None;
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, None);
        assert_eq!(value, None);
    }

    #[test]
    fn alter_with_non_empty_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Alter,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = Some(Scalar("bar".to_string()));
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Some(Ok(Phrase::Field(to_field("foo")))));
    }

    #[test]
    fn default_with_unset_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Default,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = None;
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Some(Ok(Phrase::Field(to_field("foo")))));
    }

    #[test]
    fn default_with_non_empty_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Default,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = Some(Scalar("bar".to_string()));
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, None);
        assert_eq!(value, Some(Scalar("bar".to_string())));
    }

    #[test]
    fn assign_with_unset_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Assign,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = None;
        let location = Location::dummy("somewhere");

        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Some(Ok(Phrase::Field(to_field("foo")))));

        let var = env.inner.variables.get("var").unwrap();
        assert_eq!(var.value, Some(Value::scalar("foo")));
        assert_eq!(var.last_assigned_location, Some(location));
        assert!(!var.is_exported);
        assert_eq!(var.read_only_location, None);
    }

    #[test]
    fn assign_array_word() {
        let mut env = yash_env::Env::new_virtual();
        env.variables.positional_params_mut().value = Some(Value::array(["1", "2  2", "3"]));
        env.variables
            .get_or_new("IFS".into(), Scope::Global)
            .assign("~".into(), None)
            .unwrap();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Assign,
            condition: Unset,
            word: "\"$@\"".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = None;
        let location = Location::dummy("somewhere");

        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();

        fn quoting(value: char) -> AttrChar {
            AttrChar {
                value,
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: true,
            }
        }
        fn quoted(value: char) -> AttrChar {
            AttrChar {
                value,
                origin: Origin::SoftExpansion,
                is_quoted: true,
                is_quoting: false,
            }
        }
        assert_eq!(
            result,
            Some(Ok(Phrase::Full(vec![
                vec![quoting('"'), quoted('1'), quoting('"')],
                vec![
                    quoting('"'),
                    quoted('2'),
                    quoted(' '),
                    quoted(' '),
                    quoted('2'),
                    quoting('"'),
                ],
                vec![quoting('"'), quoted('3'), quoting('"')],
            ])))
        );

        let var = env.inner.variables.get("var").unwrap();
        assert_eq!(var.value, Some(Value::scalar("1~2  2~3")));
        assert_eq!(var.last_assigned_location, Some(location));
        assert!(!var.is_exported);
        assert_eq!(var.read_only_location, None);
    }

    // TODO assign_with_array_index

    #[test]
    fn assign_with_non_empty_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Assign,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = Some(Scalar("bar".to_string()));
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, None);
        assert_eq!(value, Some(Scalar("bar".to_string())));
    }

    #[test]
    fn assign_with_read_only_variable() {
        let mut env = yash_env::Env::new_virtual();
        let mut var = env.variables.get_or_new("var".into(), Scope::Global);
        var.assign("".into(), None).unwrap();
        var.make_read_only(Location::dummy("read-only"));
        let save_var = var.clone();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Assign,
            condition: UnsetOrEmpty,
            word: "foo".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = None;
        let location = Location::dummy("somewhere");

        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        assert_matches!(result, Some(Err(error)) => {
            assert_eq!(error.location, location);
            assert_matches!(error.cause, ErrorCause::AssignError(e) => {
                assert_eq!(e.new_value, Value::scalar("foo"));
                assert_eq!(e.read_only_location, Location::dummy("read-only"));
                assert_eq!(e.assigned_location, Some(location));
            });
        });
        assert_eq!(env.inner.variables.get("var"), Some(&save_var));
    }

    #[test]
    fn assign_to_special_parameter() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Assign,
            condition: UnsetOrEmpty,
            word: "foo".parse().unwrap(),
        };
        let name = Some(Name::Special('-'));
        let mut value = None;
        let location = Location::dummy("somewhere");

        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        let error = result.unwrap().unwrap_err();
        assert_eq!(
            error.cause,
            ErrorCause::NonassignableParameter(NonassignableError::NotVariable)
        );
        assert_eq!(error.location, location);
    }

    #[test]
    fn error_with_unset_value_and_non_empty_word() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Error,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = None;
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        let error = result.unwrap().unwrap_err();
        assert_matches!(error.cause, ErrorCause::EmptyExpansion(e) => {
            assert_eq!(e.message, Some("foo".to_string()));
            assert_eq!(e.state, ValueState::Unset);
        });
    }

    #[test]
    fn error_with_empty_scalar_and_non_empty_word() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Error,
            condition: UnsetOrEmpty,
            word: "bar".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = Some(Value::scalar(""));
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        let error = result.unwrap().unwrap_err();
        assert_matches!(error.cause, ErrorCause::EmptyExpansion(e) => {
            assert_eq!(e.message, Some("bar".to_string()));
            assert_eq!(e.state, ValueState::EmptyScalar);
        });
    }

    #[test]
    fn error_with_valueless_array_and_empty_word() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Error,
            condition: UnsetOrEmpty,
            word: "".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = Some(Value::Array(vec![]));
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        let error = result.unwrap().unwrap_err();
        assert_matches!(error.cause, ErrorCause::EmptyExpansion(e) => {
            assert_eq!(e.message, None);
            assert_eq!(e.state, ValueState::ValuelessArray);
        });
        assert_eq!(error.location, location);
    }

    #[test]
    fn error_with_set_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Error,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let name = Some(Name::Variable("var"));
        let mut value = Some(Value::scalar(""));
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, name, &mut value, &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, None);
        assert_eq!(value, Some(Scalar("".to_string())));
    }
}
