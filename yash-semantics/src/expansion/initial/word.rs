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
use super::QuickExpand::{self, Ready};
use async_trait::async_trait;
use yash_syntax::syntax::Word;
use yash_syntax::syntax::WordUnit::{self, *};

fn expand_single_quote(value: &str) -> Phrase {
    let mut field = Vec::with_capacity(value.chars().count() + 2);
    let quote = AttrChar {
        value: '\'',
        origin: Origin::Literal,
        is_quoted: false,
        is_quoting: true,
    };
    field.push(quote);
    field.extend(value.chars().map(|c| AttrChar {
        value: c,
        origin: Origin::Literal,
        is_quoted: true,
        is_quoting: false,
    }));
    field.push(quote);
    Phrase::Field(field)
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
            DoubleQuote(_text) => todo!(),
            Tilde(_name) => todo!(),
        }
    }

    async fn async_expand(&self, env: &mut Env<'_>, (): ()) -> Result<Phrase, Error> {
        match self {
            Unquoted(text_unit) => text_unit.async_expand(env, ()).await,
            SingleQuote(_value) => unimplemented!("async_expand not expecting SingleQuote"),
            DoubleQuote(_text) => todo!(),
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
    use super::*;
    use assert_matches::assert_matches;

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
}
