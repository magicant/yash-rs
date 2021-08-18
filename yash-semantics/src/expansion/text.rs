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

//! Initial expansion of text.

use super::AttrChar;
use super::Env;
use super::Expander;
use super::Expansion;
use super::Origin;
use super::Result;
use super::Word;
use async_trait::async_trait;
use yash_syntax::syntax::Text;
use yash_syntax::syntax::TextUnit;

#[async_trait(?Send)]
impl Word for TextUnit {
    async fn expand<E: Env>(&self, e: &mut Expander<'_, E>) -> Result {
        use TextUnit::*;
        match self {
            Literal(c) => e.push_char(AttrChar {
                value: *c,
                origin: Origin::Literal,
                // TODO is_quoted may be true depending on the context
                is_quoted: false,
                is_quoting: false,
            }),
            // TODO Expand Backslashed correctly
            // TODO Expand RawParam correctly
            // TODO Expand BracedParam correctly
            // TODO Expand CommandSubst correctly
            // TODO Expand Backquote correctly
            // TODO Expand Arith correctly
            _ => e.push_str(&self.to_string(), Origin::Literal, false, false),
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl Word for Text {
    async fn expand<E: Env>(&self, e: &mut Expander<'_, E>) -> Result {
        for text_unit in &self.0 {
            text_unit.expand(e).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::AttrField;
    use super::*;
    use futures_executor::block_on;
    use yash_syntax::syntax::TextUnit;

    #[derive(Debug)]
    struct NullEnv;

    impl Env for NullEnv {}

    #[test]
    fn literal_expand_unquoted() {
        let mut field = AttrField::default();
        let mut env = NullEnv;
        let mut e = Expander::new(&mut env, &mut field);
        let l = TextUnit::Literal('&');
        block_on(l.expand(&mut e)).unwrap();
        assert_eq!(
            field.0,
            [AttrChar {
                value: '&',
                origin: Origin::Literal,
                is_quoted: false,
                is_quoting: false
            }]
        );
    }

    #[test]
    fn text_expand() {
        let mut field = AttrField::default();
        let mut env = NullEnv;
        let mut e = Expander::new(&mut env, &mut field);
        let text: Text = "<->".parse().unwrap();
        block_on(text.expand(&mut e)).unwrap();
        assert_eq!(
            field.0,
            [
                AttrChar {
                    value: '<',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                },
                AttrChar {
                    value: '-',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                },
                AttrChar {
                    value: '>',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                }
            ]
        );
    }
}
