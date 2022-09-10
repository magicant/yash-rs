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

//! Initial expansion of words and word units.

use super::super::attr::AttrChar;
use super::super::attr::Origin;
use super::super::Error;
use super::Env;
use super::Expand;
use super::Phrase;
use super::QuickExpand::{self, Interim, Ready};
use async_trait::async_trait;
use yash_syntax::syntax::Word;
use yash_syntax::syntax::WordUnit::{self, *};

fn expand_single_quote(value: &str) -> Phrase {
    const QUOTE: AttrChar = AttrChar {
        value: '\'',
        origin: Origin::Literal,
        is_quoted: false,
        is_quoting: true,
    };
    let mut field = Vec::with_capacity(value.chars().count() + 2);
    field.push(QUOTE);
    field.extend(value.chars().map(|c| AttrChar {
        value: c,
        origin: Origin::Literal,
        is_quoted: true,
        is_quoting: false,
    }));
    field.push(QUOTE);
    Phrase::Field(field)
}

/// Add double quotes around each field in the phrase.
///
/// This function sets the `is_quoted` flag of the characters in the phrase.
fn double_quote(phrase: &mut Phrase) {
    const QUOTE: AttrChar = AttrChar {
        value: '"',
        origin: Origin::Literal,
        is_quoted: false,
        is_quoting: true,
    };

    fn quote_field(chars: &mut Vec<AttrChar>) {
        for c in chars.iter_mut() {
            c.is_quoted = true;
        }
        chars.reserve_exact(2);
        chars.insert(0, QUOTE);
        chars.push(QUOTE);
    }

    match phrase {
        Phrase::Char(c) => {
            let is_quoted = true;
            let c = AttrChar { is_quoted, ..*c };
            *phrase = Phrase::Field(vec![QUOTE, c, QUOTE]);
        }
        Phrase::Field(chars) => quote_field(chars),
        Phrase::Full(fields) => fields.iter_mut().for_each(quote_field),
    }
}

/// Expands the word unit.
///
/// TODO Elaborate
#[async_trait(?Send)]
impl Expand for WordUnit {
    type Interim = ();

    fn quick_expand(&self, env: &mut Env<'_>) -> QuickExpand<()> {
        match self {
            Unquoted(text_unit) => text_unit.quick_expand(env),
            SingleQuote(value) => Ready(Ok(expand_single_quote(value))),
            // TODO Can we call text.quick_expand here?
            DoubleQuote(_text) => Interim(()),
            Tilde(_name) => todo!(),
        }
    }

    async fn async_expand(&self, env: &mut Env<'_>, (): ()) -> Result<Phrase, Error> {
        match self {
            Unquoted(text_unit) => text_unit.async_expand(env, ()).await,
            SingleQuote(_value) => unimplemented!("async_expand not expecting SingleQuote"),
            DoubleQuote(text) => {
                let would_split = std::mem::replace(&mut env.will_split, false);
                let result = match text.quick_expand(env) {
                    Ready(result) => result,
                    Interim(interim) => text.async_expand(env, interim).await,
                };
                env.will_split = would_split;

                let mut phrase = result?;
                double_quote(&mut phrase);
                Ok(phrase)
            }
            Tilde(_name) => todo!(),
        }
    }
}

/// Expands a word.
///
/// This implementation delegates to `[WordUnit] as Expand`.
#[async_trait(?Send)]
impl Expand for Word {
    type Interim = <[WordUnit] as Expand>::Interim;

    #[inline]
    fn quick_expand(&self, env: &mut Env<'_>) -> QuickExpand<Self::Interim> {
        self.units.quick_expand(env)
    }

    #[inline]
    async fn async_expand(
        &self,
        env: &mut Env<'_>,
        interim: Self::Interim,
    ) -> Result<Phrase, Error> {
        self.units.async_expand(env, interim).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::param::tests::env_with_positional_params_and_ifs;
    use super::super::param::tests::param;
    use super::*;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use yash_syntax::syntax::Text;
    use yash_syntax::syntax::TextUnit;

    #[test]
    fn double_quote_char() {
        let mut phrase = Phrase::Char(AttrChar {
            value: 'C',
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        });
        double_quote(&mut phrase);
        let quote = AttrChar {
            value: '"',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let c = AttrChar {
            value: 'C',
            origin: Origin::SoftExpansion,
            is_quoted: true,
            is_quoting: false,
        };
        assert_eq!(phrase, Phrase::Field(vec![quote, c, quote]));
    }

    #[test]
    fn double_quote_field() {
        let mut phrase = Phrase::Field(vec![]);
        double_quote(&mut phrase);
        let quote = AttrChar {
            value: '"',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        assert_eq!(phrase, Phrase::Field(vec![quote, quote]));

        let i = AttrChar {
            value: 'i',
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        };
        let f = AttrChar {
            value: 'f',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        phrase = Phrase::Field(vec![i, f]);
        double_quote(&mut phrase);
        let is_quoted = true;
        let i = AttrChar { is_quoted, ..i };
        let f = AttrChar { is_quoted, ..f };
        assert_eq!(phrase, Phrase::Field(vec![quote, i, f, quote]));
    }

    #[test]
    fn double_quote_full() {
        let mut phrase = Phrase::Full(vec![]);
        double_quote(&mut phrase);
        assert_eq!(phrase, Phrase::zero_fields());

        let a = AttrChar {
            value: 'a',
            origin: Origin::HardExpansion,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar { value: 'b', ..a };
        phrase = Phrase::Full(vec![vec![a], vec![b]]);
        double_quote(&mut phrase);
        let quote = AttrChar {
            value: '"',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let is_quoted = true;
        let a = AttrChar { is_quoted, ..a };
        let b = AttrChar { is_quoted, ..b };
        assert_eq!(
            phrase,
            Phrase::Full(vec![vec![quote, a, quote], vec![quote, b, quote]])
        );
    }

    #[test]
    fn unquoted() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let unit: WordUnit = "x".parse().unwrap();
        assert_matches!(unit.quick_expand(&mut env), Ready(result) => {
            let c = AttrChar {
                value: 'x',
                origin: Origin::Literal,
                is_quoted: false,
                is_quoting: false,
            };
            assert_eq!(result, Ok(Phrase::Char(c)));
        });
    }

    #[test]
    fn empty_single_quote() {
        let result = expand_single_quote("");
        let q = AttrChar {
            value: '\'',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        assert_eq!(result, Phrase::Field(vec![q, q]));
    }

    #[test]
    fn non_empty_single_quote() {
        let result = expand_single_quote("do");
        let q = AttrChar {
            value: '\'',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let d = AttrChar {
            value: 'd',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: false,
        };
        let o = AttrChar { value: 'o', ..d };
        assert_eq!(result, Phrase::Field(vec![q, d, o, q]));
    }

    #[test]
    fn async_double_quote() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let unit = DoubleQuote(Text(vec![TextUnit::Literal('X')]));
        assert_matches!(unit.quick_expand(&mut env), Interim(()));
        let result = unit.async_expand(&mut env, ()).now_or_never().unwrap();
        let quote = AttrChar {
            value: '"',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let x = AttrChar {
            value: 'X',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: false,
        };
        assert_eq!(result, Ok(Phrase::Field(vec![quote, x, quote])));
    }

    #[test]
    fn inside_double_quote_is_non_splitting_context() {
        let mut env = env_with_positional_params_and_ifs();
        let mut env = Env::new(&mut env);
        let unit = DoubleQuote(Text(vec![TextUnit::BracedParam(param("*"))]));
        assert_matches!(unit.quick_expand(&mut env), Interim(()));
        let result = unit.async_expand(&mut env, ()).now_or_never().unwrap();

        assert!(env.will_split);
        let quote = AttrChar {
            value: '"',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let a = AttrChar {
            value: 'a',
            origin: Origin::SoftExpansion,
            is_quoted: true,
            is_quoting: false,
        };
        let amp = AttrChar { value: '&', ..a };
        let c = AttrChar { value: 'c', ..a };
        assert_eq!(result, Ok(Phrase::Field(vec![quote, a, amp, c, quote])));
    }
}
