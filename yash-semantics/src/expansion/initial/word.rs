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

use super::super::Error;
use super::super::attr::AttrChar;
use super::super::attr::Origin;
use super::Env;
use super::Expand;
use super::Phrase;
use crate::Runtime;
use yash_syntax::syntax::Unquote as _;
use yash_syntax::syntax::Word;
use yash_syntax::syntax::WordUnit::{self, *};

const SINGLE_QUOTE: AttrChar = AttrChar {
    value: '\'',
    origin: Origin::Literal,
    is_quoted: false,
    is_quoting: true,
};

/// Adds single quotes around the string.
fn single_quote(value: &str) -> Phrase {
    let mut field = Vec::with_capacity(value.chars().count() + 2);
    field.push(SINGLE_QUOTE);
    field.extend(value.chars().map(|c| AttrChar {
        value: c,
        origin: Origin::Literal,
        is_quoted: true,
        is_quoting: false,
    }));
    field.push(SINGLE_QUOTE);
    Phrase::Field(field)
}

/// Adds dollar-single-quotes around the string.
fn dollar_single_quote(s: &str) -> Phrase {
    const DOLLAR: AttrChar = AttrChar {
        value: '$',
        origin: Origin::Literal,
        is_quoted: false,
        is_quoting: true,
    };
    let mut field = Vec::with_capacity(s.chars().count() + 3);
    field.push(DOLLAR);
    field.push(SINGLE_QUOTE);
    field.extend(s.chars().map(|c| AttrChar {
        value: c,
        origin: Origin::Literal,
        is_quoted: true,
        is_quoting: false,
    }));
    field.push(SINGLE_QUOTE);
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
/// # Unquoted
///
/// Expansion of `Unquoted(text_unit)` delegates to expansion of `text_unit`.
///
/// # Single quote
///
/// `SingleQuote(value)` expands to `value` surrounded by `'`.
///
/// # Double quote
///
/// A double-quoted text expands to a phrase in a non-splitting context and
/// surrounds each field in the phrase with `"`.
///
/// # Dollar-single-quote
///
/// `DollarSingleQuote(string)` expands to
/// `dollar_single_quote(&string.unquote().0)` surrounded by `$'` and `'`.
///
/// # Tilde
///
/// `Tilde("")` expands to the value of the `HOME` scalar variable.
///
/// `Tilde(user)` expands to the `user`'s home directory.
///
/// TODO: `~+`, `~-`, `~+n`, `~-n`
///
/// In all cases, if the result would be empty, it expands to a dummy quote to
/// prevent it from being removed in field splitting. The quote is expected to
/// be removed by quote removal.
impl<S: Runtime + 'static> Expand<S> for WordUnit {
    async fn expand(&self, env: &mut Env<'_, S>) -> Result<Phrase, Error> {
        match self {
            Unquoted(text_unit) => text_unit.expand(env).await,
            SingleQuote(value) => Ok(single_quote(value)),
            DoubleQuote(text) => {
                let would_split = std::mem::replace(&mut env.will_split, false);
                let result = text.expand(env).await;
                env.will_split = would_split;

                let mut phrase = result?;
                double_quote(&mut phrase);
                Ok(phrase)
            }
            DollarSingleQuote(string) => Ok(dollar_single_quote(&string.unquote().0)),
            Tilde {
                name,
                followed_by_slash,
            } => Ok(super::tilde::expand(name, *followed_by_slash, env.inner).into()),
        }
    }
}

/// Expands a word.
///
/// This implementation delegates to `[WordUnit] as Expand`.
impl<S: Runtime + 'static> Expand<S> for Word {
    #[inline]
    async fn expand(&self, env: &mut Env<'_, S>) -> Result<Phrase, Error> {
        self.units.expand(env).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::param::tests::braced_param;
    use super::super::param::tests::env_with_positional_params_and_ifs;
    use super::*;
    use futures_util::FutureExt;
    use yash_syntax::syntax::SpecialParam;
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
        let result = unit.expand(&mut env).now_or_never().unwrap();

        let c = AttrChar {
            value: 'x',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        assert_eq!(result, Ok(Phrase::Char(c)));
    }

    #[test]
    fn empty_single_quote() {
        let result = single_quote("");
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
        let result = single_quote("do");
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
    fn expand_dollar_single_quote() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let unit = DollarSingleQuote(r"\\\n".parse().unwrap());
        let result = unit.expand(&mut env).now_or_never().unwrap();

        let dollar = AttrChar {
            value: '$',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let quote = AttrChar {
            value: '\'',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let backslash = AttrChar {
            value: '\\',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: false,
        };
        let newline = AttrChar {
            value: '\n',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: false,
        };
        assert_eq!(
            result,
            Ok(Phrase::Field(vec![
                dollar, quote, backslash, newline, quote
            ]))
        );
    }

    #[test]
    fn expand_double_quote() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let unit = DoubleQuote(Text(vec![TextUnit::Literal('X')]));
        let result = unit.expand(&mut env).now_or_never().unwrap();

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
        let unit = TextUnit::BracedParam(braced_param(SpecialParam::Asterisk));
        let unit = DoubleQuote(Text(vec![unit]));
        let result = unit.expand(&mut env).now_or_never().unwrap();

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
