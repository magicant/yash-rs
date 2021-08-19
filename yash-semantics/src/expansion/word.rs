// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Initial expansion of word.

use super::Env;
use super::Expand;
use super::Expander;
use super::Expansion;
use super::Origin;
use super::Result;
use async_trait::async_trait;
use yash_syntax::syntax::Word;
use yash_syntax::syntax::WordUnit;

#[async_trait(?Send)]
impl Expand for WordUnit {
    async fn expand<E: Env>(&self, e: &mut Expander<'_, E>) -> Result {
        use WordUnit::*;
        match self {
            Unquoted(text_unit) => text_unit.expand(e).await,
            // TODO Expand Tilde correctly
            // TODO Expand SingleQuote correctly
            // TODO Expand DoubleQuote correctly
            _ => {
                e.push_str(&self.to_string(), Origin::Literal, false, false);
                Ok(())
            }
        }
    }
}

#[async_trait(?Send)]
impl Expand for Word {
    async fn expand<E: Env>(&self, e: &mut Expander<'_, E>) -> Result {
        self.units.expand(e).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::AttrChar;
    use super::super::AttrField;
    use super::*;
    use futures_executor::block_on;

    #[derive(Debug)]
    struct NullEnv;

    impl Env for NullEnv {}

    #[test]
    fn unquoted_expand() {
        let mut field = AttrField::default();
        let mut env = NullEnv;
        let mut e = Expander::new(&mut env, &mut field);
        let u: WordUnit = "x".parse().unwrap();
        block_on(u.expand(&mut e)).unwrap();
        assert_eq!(
            field.0,
            [AttrChar {
                value: 'x',
                origin: Origin::Literal,
                is_quoted: false,
                is_quoting: false
            }]
        );
    }

    #[test]
    fn word_expand() {
        let mut field = AttrField::default();
        let mut env = NullEnv;
        let mut e = Expander::new(&mut env, &mut field);
        let w: Word = "xyz".parse().unwrap();
        block_on(w.expand(&mut e)).unwrap();
        assert_eq!(
            field.0,
            [
                AttrChar {
                    value: 'x',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                },
                AttrChar {
                    value: 'y',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                },
                AttrChar {
                    value: 'z',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                }
            ]
        );
    }
}
