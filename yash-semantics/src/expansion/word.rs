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

use super::AttrChar;
use super::AttrField;
use super::Env;
use super::Expand;
use super::ExpandToField;
use super::Expansion;
use super::Origin;
use super::Output;
use super::Result;
use async_trait::async_trait;
use yash_syntax::syntax::Word;
use yash_syntax::syntax::WordUnit;

#[async_trait(?Send)]
impl Expand for WordUnit {
    /// Expands the word unit.
    ///
    /// TODO Elaborate
    async fn expand<E: Env>(&self, env: &mut E, output: &mut Output<'_>) -> Result {
        use WordUnit::*;
        match self {
            Unquoted(text_unit) => text_unit.expand(env, output).await,
            SingleQuote(string) => {
                let quote = AttrChar {
                    value: '\'',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: true,
                };
                output.push_char(quote);
                output.push_str(string, Origin::Literal, true, false);
                output.push_char(quote);
                Ok(())
            }
            DoubleQuote(text) => {
                let quote = AttrChar {
                    value: '"',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: true,
                };
                output.push_char(quote);
                {
                    let mut output = output.begin_quote();
                    text.expand(env, &mut output).await?;
                    Output::end_quote(output);
                }
                output.push_char(quote);
                Ok(())
            }
            // TODO Expand Tilde correctly
            _ => {
                output.push_str(&self.to_string(), Origin::Literal, false, false);
                Ok(())
            }
        }
    }
}

#[async_trait(?Send)]
impl Expand for Word {
    /// Expands the word.
    async fn expand<E: Env>(&self, env: &mut E, output: &mut Output<'_>) -> Result {
        self.units.expand(env, output).await
    }
}

#[async_trait(?Send)]
impl ExpandToField for Word {
    async fn expand_to_field<E: Env>(&self, env: &mut E) -> Result<AttrField> {
        let mut chars = Vec::new();
        self.units.expand(env, &mut Output::new(&mut chars)).await?;
        let origin = self.location.clone();
        Ok(AttrField { chars, origin })
    }

    async fn expand_to_fields_into<E: Env, F: Extend<AttrField>>(
        &self,
        env: &mut E,
        fields: &mut F,
    ) -> Result {
        let mut fields_without_origin = Vec::new();
        self.units
            .expand(env, &mut Output::new(&mut fields_without_origin))
            .await?;
        fields.extend(fields_without_origin.into_iter().map(|chars| AttrField {
            chars,
            origin: self.location.clone(),
        }));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expansion::tests::NullEnv;
    use futures_executor::block_on;

    #[test]
    fn unquoted_expand() {
        let mut field = Vec::<AttrChar>::default();
        let mut env = NullEnv;
        let mut output = Output::new(&mut field);
        let u: WordUnit = "x".parse().unwrap();
        block_on(u.expand(&mut env, &mut output)).unwrap();
        assert_eq!(
            field,
            [AttrChar {
                value: 'x',
                origin: Origin::Literal,
                is_quoted: false,
                is_quoting: false
            }]
        );
    }

    #[test]
    fn single_quote_expand() {
        let mut field = Vec::<AttrChar>::default();
        let mut env = NullEnv;
        let mut output = Output::new(&mut field);
        let u = WordUnit::SingleQuote("ex".to_string());
        block_on(u.expand(&mut env, &mut output)).unwrap();
        assert_eq!(
            field,
            [
                AttrChar {
                    value: '\'',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: true,
                },
                AttrChar {
                    value: 'e',
                    origin: Origin::Literal,
                    is_quoted: true,
                    is_quoting: false,
                },
                AttrChar {
                    value: 'x',
                    origin: Origin::Literal,
                    is_quoted: true,
                    is_quoting: false,
                },
                AttrChar {
                    value: '\'',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: true,
                },
            ]
        );
    }

    #[test]
    fn double_quote_expand() {
        let mut field = Vec::<AttrChar>::default();
        let mut env = NullEnv;
        let mut output = Output::new(&mut field);
        let u: WordUnit = r#""\a\$""#.parse().unwrap();
        block_on(u.expand(&mut env, &mut output)).unwrap();

        let quote = AttrChar {
            value: '"',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        assert_eq!(
            field,
            [
                quote,
                AttrChar {
                    value: '\\',
                    origin: Origin::Literal,
                    is_quoted: true,
                    is_quoting: false,
                },
                AttrChar {
                    value: 'a',
                    origin: Origin::Literal,
                    is_quoted: true,
                    is_quoting: false,
                },
                AttrChar {
                    value: '\\',
                    origin: Origin::Literal,
                    is_quoted: true,
                    is_quoting: true,
                },
                AttrChar {
                    value: '$',
                    origin: Origin::Literal,
                    is_quoted: true,
                    is_quoting: false,
                },
                quote,
            ]
        );
    }

    #[test]
    fn word_expand() {
        let mut field = Vec::<AttrChar>::default();
        let mut env = NullEnv;
        let mut output = Output::new(&mut field);
        let w: Word = "xyz".parse().unwrap();
        block_on(w.expand(&mut env, &mut output)).unwrap();
        assert_eq!(
            field,
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

    #[test]
    fn word_expand_to_field() {
        let mut env = NullEnv;
        let w: Word = "abc".parse().unwrap();
        let result = block_on(w.expand_to_field(&mut env)).unwrap();
        assert_eq!(
            result.chars,
            [
                AttrChar {
                    value: 'a',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                },
                AttrChar {
                    value: 'b',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                },
                AttrChar {
                    value: 'c',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                }
            ]
        );
        assert_eq!(result.origin, w.location);
    }

    #[test]
    fn word_expand_to_fields() {
        let mut env = NullEnv;
        let w: Word = "abc".parse().unwrap();
        let result = block_on(w.expand_to_fields(&mut env)).unwrap();
        assert_eq!(result.len(), 1, "{:?}", result);
        assert_eq!(
            result[0].chars,
            [
                AttrChar {
                    value: 'a',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                },
                AttrChar {
                    value: 'b',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                },
                AttrChar {
                    value: 'c',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                }
            ]
        );
        assert_eq!(result[0].origin, w.location);
        // TODO Test with a word that expands to more than one field
    }
}
