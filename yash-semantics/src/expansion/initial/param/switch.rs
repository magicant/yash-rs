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
use super::to_field;
use crate::expansion::AssignReadOnlyError;
use crate::expansion::ErrorCause;
use crate::expansion::attr::Origin;
use crate::expansion::attr_strip::Strip;
use crate::expansion::expand_word;
use crate::expansion::initial::Expand as _;
use crate::expansion::quote_removal::skip_quotes;
use yash_env::variable::Scope;
use yash_env::variable::Value;
use yash_syntax::source::Location;
use yash_syntax::syntax::Param;
use yash_syntax::syntax::ParamType;
use yash_syntax::syntax::Switch;
use yash_syntax::syntax::SwitchCondition;
use yash_syntax::syntax::SwitchType;
use yash_syntax::syntax::Word;

/// Subdivision of [value](Value) states that may be considered as "not set"
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Vacancy {
    /// The variable is not set.
    Unset,
    /// The value is a scalar with no characters.
    EmptyScalar,
    /// The value is an array with no elements.
    ValuelessArray,
    /// The value is an array with one element containing no characters.
    EmptyValueArray,
}

impl Vacancy {
    /// Evaluates the vacancy of a value.
    ///
    /// Returns `None` if the value does not fall into any of the `Vacancy`
    /// categories.
    #[inline]
    #[must_use]
    pub fn of<'a, I: Into<Option<&'a Value>>>(value: I) -> Option<Vacancy> {
        fn inner(value: Option<&Value>) -> Option<Vacancy> {
            use Vacancy::*;
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
        use Vacancy::*;
        match self {
            Unset => "unset variable",
            EmptyScalar => "empty string",
            ValuelessArray => "empty array",
            EmptyValueArray => "array with empty string",
        }
    }
}

impl std::fmt::Display for Vacancy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.description().fmt(f)
    }
}

/// Error caused by a [`Switch`] of [`SwitchType::Error`]
///
/// `VacantError` is an error that is returned when you apply an error switch to
/// a [vacant](Vacancy) value.
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
#[error("{} ({}: {})", self.message_or_default(), .param, .vacancy)]
#[non_exhaustive]
pub struct VacantError {
    /// Parameter that caused this error
    pub param: Param,
    /// State of the parameter value that caused this error
    pub vacancy: Vacancy,
    /// Error message specified in the switch
    pub message: Option<String>,
}

impl VacantError {
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
#[error("{cause}")]
pub struct NonassignableError {
    /// Cause of the error
    pub cause: NonassignableErrorCause,
    /// State of the parameter value that caused an attempt to assign an
    /// alternate value that resulted in this error
    pub vacancy: Vacancy,
}

#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
#[non_exhaustive]
pub enum NonassignableErrorCause {
    /// The parameter is not a variable.
    #[error("parameter `{param}` is not an assignable variable")]
    NotVariable { param: Param },
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
    Occupied,
    Vacant(Vacancy),
}

impl ValueCondition {
    fn with<V: Into<Option<Vacancy>>>(cond: SwitchCondition, vacancy: V) -> Self {
        fn inner(cond: SwitchCondition, vacancy: Option<Vacancy>) -> ValueCondition {
            match (cond, vacancy) {
                (_, None) => ValueCondition::Occupied,

                (SwitchCondition::UnsetOrEmpty, Some(vacancy)) => ValueCondition::Vacant(vacancy),

                (_, Some(Vacancy::Unset)) => ValueCondition::Vacant(Vacancy::Unset),

                (
                    SwitchCondition::Unset,
                    Some(Vacancy::EmptyScalar | Vacancy::ValuelessArray | Vacancy::EmptyValueArray),
                ) => ValueCondition::Occupied,
            }
        }
        inner(cond, vacancy.into())
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
///
/// As specified in the POSIX standard, this function expands the `value` and
/// performs quote removal. The result is assigned to the variable `name` in the
/// global scope and returned as a [`Phrase`].
async fn assign(
    env: &mut Env<'_>,
    param: &Param,
    vacancy: Vacancy,
    value: &Word,
    location: Location,
) -> Result<Phrase, Error> {
    // TODO Support assignment to an array element
    if param.r#type != ParamType::Variable {
        let param = param.clone();
        let cause = NonassignableErrorCause::NotVariable { param };
        let cause = ErrorCause::NonassignableParameter(NonassignableError { cause, vacancy });
        return Err(Error { cause, location });
    }
    let value_phrase = attribute(value.expand(env).await?);
    let joined_value = value_phrase.ifs_join(&env.inner.variables);
    let final_value = skip_quotes(joined_value).strip().collect::<String>();
    let result = to_field(&final_value).into();
    env.inner
        .get_or_create_variable(&param.id, Scope::Global)
        .assign(final_value, location)
        .map_err(|e| {
            let location = e.assigned_location.unwrap();
            let cause = ErrorCause::AssignReadOnly(AssignReadOnlyError {
                name: param.id.to_owned(),
                new_value: e.new_value,
                read_only_location: e.read_only_location,
                vacancy: Some(vacancy),
            });
            Error { cause, location }
        })?;
    Ok(result)
}

/// Expands a word to be used as a vacant expansion error message.
async fn vacant_expansion_error_message(
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

/// Constructs a vacant expansion error.
async fn vacant_expansion_error(
    env: &mut Env<'_>,
    param: &Param,
    vacancy: Vacancy,
    message_word: &Word,
    location: Location,
) -> Error {
    let message = match vacant_expansion_error_message(env, message_word).await {
        Ok(message) => message,
        Err(error) => return error,
    };
    let cause = ErrorCause::VacantExpansion(VacantError {
        param: param.clone(),
        vacancy,
        message,
    });
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
    param: &Param,
    value: Option<&Value>,
    location: &Location,
) -> Option<Result<Phrase, Error>> {
    use SwitchType::*;
    use ValueCondition::*;
    let cond = ValueCondition::with(switch.condition, Vacancy::of(value));
    match (switch.r#type, cond) {
        (Alter, Vacant(_)) | (Default, Occupied) | (Assign, Occupied) | (Error, Occupied) => None,

        (Alter, Occupied) | (Default, Vacant(_)) => {
            Some(switch.word.expand(env).await.map(attribute))
        }

        (Assign, Vacant(vacancy)) => {
            Some(assign(env, param, vacancy, &switch.word, location.clone()).await)
        }

        (Error, Vacant(vacancy)) => Some(Err(vacant_expansion_error(
            env,
            param,
            vacancy,
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
    use yash_env::variable::IFS;
    use yash_syntax::syntax::SpecialParam;
    use yash_syntax::syntax::SwitchCondition::*;
    use yash_syntax::syntax::SwitchType::*;

    #[test]
    fn vacancy_of_values() {
        let vacancy = Vacancy::of(&None);
        assert_eq!(vacancy, Some(Vacancy::Unset));
        let vacancy = Vacancy::of(&Some(Value::scalar("")));
        assert_eq!(vacancy, Some(Vacancy::EmptyScalar));
        let vacancy = Vacancy::of(&Some(Value::scalar(".")));
        assert_eq!(vacancy, None);
        let vacancy = Vacancy::of(&Some(Value::Array(vec![])));
        assert_eq!(vacancy, Some(Vacancy::ValuelessArray));
        let vacancy = Vacancy::of(&Some(Value::array([""])));
        assert_eq!(vacancy, Some(Vacancy::EmptyValueArray));
        let vacancy = Vacancy::of(&Some(Value::array(["."])));
        assert_eq!(vacancy, None);
        let vacancy = Vacancy::of(&Some(Value::array(["", ""])));
        assert_eq!(vacancy, None);
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
    fn alter_with_vacant_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Alter,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let param = Param::variable("var");
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, &param, None, &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn alter_with_occupied_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Alter,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let param = Param::variable("var");
        let value = Value::scalar("bar");
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, &param, Some(&value), &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Some(Ok(Phrase::Field(to_field("foo")))));
    }

    #[test]
    fn default_with_vacant_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Default,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let param = Param::variable("var");
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, &param, None, &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Some(Ok(Phrase::Field(to_field("foo")))));
    }

    #[test]
    fn default_with_occupied_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Default,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let param = Param::variable("var");
        let value = Value::scalar("bar");
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, &param, Some(&value), &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn assign_with_vacant_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Assign,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let param = Param::variable("var");
        let location = Location::dummy("somewhere");

        let result = apply(&mut env, &switch, &param, None, &location)
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
        env.variables.positional_params_mut().values =
            vec!["1".to_string(), "2  2".to_string(), "3".to_string()];
        env.variables
            .get_or_new(IFS, Scope::Global)
            .assign("~", None)
            .unwrap();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Assign,
            condition: Unset,
            word: "\"$@\"".parse().unwrap(),
        };
        let param = Param::variable("var");
        let location = Location::dummy("somewhere");

        let result = apply(&mut env, &switch, &param, None, &location)
            .now_or_never()
            .unwrap();

        fn char(value: char) -> AttrChar {
            AttrChar {
                value,
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            }
        }
        assert_eq!(
            result,
            Some(Ok(Phrase::Field(vec![
                char('1'),
                char('~'),
                char('2'),
                char(' '),
                char(' '),
                char('2'),
                char('~'),
                char('3'),
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
    fn assign_with_occupied_value() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Assign,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let param = Param::variable("var");
        let value = Value::scalar("bar");
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, &param, Some(&value), &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn assign_with_read_only_variable() {
        let mut env = yash_env::Env::new_virtual();
        let mut var = env.variables.get_or_new("var", Scope::Global);
        var.assign("", None).unwrap();
        var.make_read_only(Location::dummy("read-only"));
        let save_var = var.clone();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Assign,
            condition: UnsetOrEmpty,
            word: "foo".parse().unwrap(),
        };
        let param = Param::variable("var");
        let value = save_var.value.as_ref();
        let location = Location::dummy("somewhere");

        let result = apply(&mut env, &switch, &param, value, &location)
            .now_or_never()
            .unwrap();
        assert_matches!(result, Some(Err(error)) => {
            assert_eq!(error.location, location);
            assert_matches!(error.cause, ErrorCause::AssignReadOnly(e) => {
                assert_eq!(e.name, "var");
                assert_eq!(e.new_value, Value::scalar("foo"));
                assert_eq!(e.read_only_location, Location::dummy("read-only"));
                assert_eq!(e.vacancy, Some(Vacancy::EmptyScalar));
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
        let param = Param::from(SpecialParam::Hyphen);
        let value = Value::scalar("");
        let location = Location::dummy("somewhere");

        let result = apply(&mut env, &switch, &param, Some(&value), &location)
            .now_or_never()
            .unwrap();
        let error = result.unwrap().unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::NonassignableParameter(error) => {
                assert_eq!(error.cause, NonassignableErrorCause::NotVariable { param });
                assert_eq!(error.vacancy, Vacancy::EmptyScalar);
            }
        );
        assert_eq!(error.location, location);
    }

    #[test]
    fn error_with_vacant_value_and_non_empty_word() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let switch = Switch {
            r#type: Error,
            condition: Unset,
            word: "foo".parse().unwrap(),
        };
        let param = Param::variable("var");
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, &param, None, &location)
            .now_or_never()
            .unwrap();
        let error = result.unwrap().unwrap_err();
        assert_matches!(error.cause, ErrorCause::VacantExpansion(e) => {
            assert_eq!(e.param, param);
            assert_eq!(e.message, Some("foo".to_string()));
            assert_eq!(e.vacancy, Vacancy::Unset);
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
        let param = Param::variable("var");
        let value = Value::scalar("");
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, &param, Some(&value), &location)
            .now_or_never()
            .unwrap();
        let error = result.unwrap().unwrap_err();
        assert_matches!(error.cause, ErrorCause::VacantExpansion(e) => {
            assert_eq!(e.param, param);
            assert_eq!(e.message, Some("bar".to_string()));
            assert_eq!(e.vacancy, Vacancy::EmptyScalar);
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
        let param = Param::variable("var");
        let value = Value::Array(vec![]);
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, &param, Some(&value), &location)
            .now_or_never()
            .unwrap();
        let error = result.unwrap().unwrap_err();
        assert_matches!(error.cause, ErrorCause::VacantExpansion(e) => {
            assert_eq!(e.param, param);
            assert_eq!(e.message, None);
            assert_eq!(e.vacancy, Vacancy::ValuelessArray);
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
        let param = Param::variable("var");
        let value = Value::scalar("");
        let location = Location::dummy("somewhere");
        let result = apply(&mut env, &switch, &param, Some(&value), &location)
            .now_or_never()
            .unwrap();
        assert_eq!(result, None);
    }
}
