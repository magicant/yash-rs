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
    fn with(_cond: SwitchCondition, value: &Option<Value>) -> Self {
        // TODO Apply the condition
        match value {
            Some(_) => ValueState::Set,
            None => ValueState::Unset,
        }
    }
}

/// Modifies the origin of characters in the phrase to `SoftExpansion`.
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
        (Alter, Set) => Some(expand(env, &switch.word).await.map(attribute)),
        (Alter, Unset) => None,
        (Default, Set) => todo!(),
        (Default, Unset) => todo!(),
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
}
