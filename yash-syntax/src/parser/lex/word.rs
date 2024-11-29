// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2020 WATANABE Yuki
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

//! Part of the lexer that parses words.

use super::core::Lexer;
use super::core::WordContext;
use super::core::WordLexer;
use crate::parser::core::Result;
use crate::parser::error::Error;
use crate::parser::error::SyntaxError;
use crate::source::Location;
use crate::source::SourceChar;
use crate::syntax::TextUnit;
use crate::syntax::Word;
use crate::syntax::WordUnit::{self, DoubleQuote, SingleQuote, Unquoted};

impl Lexer<'_> {
    /// Parses a single-quoted string.
    ///
    /// The opening `'` must have been consumed before calling this function.
    /// The closing `'` is consumed in this function.
    ///
    /// `opening_location` should be the location of the opening `'`. It is used
    /// to construct an error value, but this function does not check if it
    /// actually is a location of `'`.
    async fn single_quote(&mut self, opening_location: Location) -> Result<WordUnit> {
        let mut content = String::new();
        let mut lexer = self.disable_line_continuation();
        loop {
            match lexer.consume_char_if(|_| true).await? {
                Some(&SourceChar { value: '\'', .. }) => break,
                Some(&SourceChar { value, .. }) => content.push(value),
                None => {
                    let cause = SyntaxError::UnclosedSingleQuote { opening_location }.into();
                    let location = lexer.location().await?.clone();
                    return Err(Error { cause, location });
                }
            }
        }
        Lexer::enable_line_continuation(lexer);
        Ok(SingleQuote(content))
    }

    /// Parses a double-quoted string.
    ///
    /// The opening `"` must have been consumed before calling this function.
    /// The closing `"` is consumed in this function.
    ///
    /// `opening_location` should be the location of the opening `"`. It is used
    /// to construct an error value, but this function does not check if it
    /// actually is a location of `"`.
    async fn double_quote(&mut self, opening_location: Location) -> Result<WordUnit> {
        fn is_delimiter(c: char) -> bool {
            c == '"'
        }
        fn is_escapable(c: char) -> bool {
            matches!(c, '$' | '`' | '"' | '\\')
        }

        let content = self.text(is_delimiter, is_escapable).await?;

        if self.skip_if(|c| c == '"').await? {
            Ok(DoubleQuote(content))
        } else {
            let cause = SyntaxError::UnclosedDoubleQuote { opening_location }.into();
            let location = self.location().await?.clone();
            Err(Error { cause, location })
        }
    }
}

impl WordLexer<'_, '_> {
    /// Parses a word unit.
    ///
    /// `is_delimiter` is a function that decides a character is a delimiter. An
    /// unquoted character is parsed only if `is_delimiter` returns false for it.
    ///
    /// The word context defines what characters can be escaped by a backslash.
    /// If [`self.context`](Self::context) is `Word`, any character can be
    /// escaped. If `Text`, then `$`, `"`, `` ` `` and `\` can be escaped as
    /// well as delimiters.
    ///
    /// This function does not parse tilde expansion. See [`word`](Self::word).
    pub async fn word_unit<F>(&mut self, is_delimiter: F) -> Result<Option<WordUnit>>
    where
        F: Fn(char) -> bool,
    {
        self.word_unit_dyn(&is_delimiter).await
    }

    /// Dynamic version of [`Self::word_unit`].
    async fn word_unit_dyn(
        &mut self,
        is_delimiter: &dyn Fn(char) -> bool,
    ) -> Result<Option<WordUnit>> {
        let allow_single_quote = match self.context {
            WordContext::Word => true,
            WordContext::Text => false,
        };
        let escape_all = |_| true;
        let escape_some = |c| matches!(c, '$' | '"' | '`' | '\\') || is_delimiter(c);
        let is_escapable: &dyn Fn(char) -> bool = match self.context {
            WordContext::Word => &escape_all,
            WordContext::Text => &escape_some,
        };

        match self.peek_char().await? {
            Some('\'') if allow_single_quote => {
                let location = self.location().await?.clone();
                self.consume_char();
                self.single_quote(location).await.map(Some)
            }
            Some('"') => {
                let location = self.location().await?.clone();
                self.consume_char();
                self.double_quote(location).await.map(Some)
            }
            _ => {
                let unit = self.text_unit(is_delimiter, is_escapable).await?;
                if allow_single_quote && unit == Some(TextUnit::Literal('$')) {
                    if let Some(result) = self.dollar_single_quote().await? {
                        return Ok(Some(result));
                    }
                    // TODO Maybe reject any other characters after `$`?
                }
                Ok(unit.map(Unquoted))
            }
        }
    }

    /// Parses a word token.
    ///
    /// `is_delimiter` is a function that decides which character is a
    /// delimiter. The word ends when an unquoted delimiter is found. To parse a
    /// normal word token, you should pass
    /// [`is_token_delimiter_char`](super::is_token_delimiter_char) as
    /// `is_delimiter`. Other functions can be passed to parse a word that ends
    /// with different delimiters.
    ///
    /// This function does not parse any tilde expansions in the word.
    /// To parse them, you need to call [`Word::parse_tilde_front`] or
    /// [`Word::parse_tilde_everywhere`] on the resultant word.
    pub async fn word<F>(&mut self, is_delimiter: F) -> Result<Word>
    where
        F: Fn(char) -> bool,
    {
        self.word_dyn(&is_delimiter).await
    }

    /// Dynamic version of [`Self::word`].
    async fn word_dyn(&mut self, is_delimiter: &dyn Fn(char) -> bool) -> Result<Word> {
        let start = self.index();
        let mut units = vec![];
        while let Some(unit) = self.word_unit_dyn(is_delimiter).await? {
            units.push(unit)
        }
        let location = self.location_range(start..self.index());
        Ok(Word { units, location })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorCause;
    use crate::parser::lex::WordContext;
    use crate::source::Source;
    use crate::syntax::EscapeUnit;
    use crate::syntax::EscapedString;
    use crate::syntax::Modifier;
    use crate::syntax::Text;
    use crate::syntax::TextUnit::{Backslashed, BracedParam, CommandSubst, Literal};
    use crate::syntax::WordUnit::{DollarSingleQuote, Tilde};
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn lexer_word_unit_unquoted() {
        let mut lexer = Lexer::from_memory("$()", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, Unquoted(CommandSubst { content, location }) => {
            assert_eq!(&*content, "");
            assert_eq!(location.range, 0..3);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_unquoted_escapes_in_word_context() {
        // Any characters can be escaped in this context.
        let mut lexer = Lexer::from_memory(r#"\a\$\`\"\\\'\#\{\}"#, Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('a')))));
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('$')))));
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('`')))));
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('"')))));
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('\\')))));
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('\'')))));
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('#')))));
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('{')))));
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('}')))));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_unquoted_escapes_in_text_context() {
        // $, `, " and \ can be escaped as well as delimiters
        let mut lexer = Lexer::from_memory(r#"\a\$\`\"\\\'\#\{\}"#, Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Text,
        };
        let is_delimiter = |c| c == '}';

        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Literal('\\')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Literal('a')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('$')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('`')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('"')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('\\')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Literal('\\')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Literal('\'')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Literal('\\')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Literal('#')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Literal('\\')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Literal('{')))));
        let result = lexer.word_unit(is_delimiter).now_or_never().unwrap();
        assert_eq!(result, Ok(Some(Unquoted(Backslashed('}')))));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_orphan_dollar_is_literal() {
        let mut lexer = Lexer::from_memory("$", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .word_unit(|c| {
                assert_eq!(c, '$', "unexpected call to is_delimiter({c:?})");
                false
            })
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(result, Unquoted(Literal('$')));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_single_quote_empty() {
        let mut lexer = Lexer::from_memory("''", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, SingleQuote(content) => assert_eq!(content, ""));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_single_quote_nonempty() {
        let mut lexer = Lexer::from_memory("'abc\\\n$def\\'", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, SingleQuote(content) => assert_eq!(content, "abc\\\n$def\\"));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_single_quote_unclosed() {
        let mut lexer = Lexer::from_memory("'abc\ndef\\", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let e = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedSingleQuote { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "'abc\ndef\\");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(*opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 0..1);
        });
        assert_eq!(*e.location.code.value.borrow(), "'abc\ndef\\");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 9..9);
    }

    #[test]
    fn lexer_word_unit_not_single_quote_in_text_context() {
        let mut lexer = Lexer::from_memory("'", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Text,
        };

        let result = lexer
            .word_unit(|c| {
                assert_eq!(c, '\'', "unexpected call to is_delimiter({c:?})");
                false
            })
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(result, Unquoted(Literal('\'')));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_dollar_single_quote_empty() {
        let mut lexer = Lexer::from_memory("$''", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .word_unit(|c| {
                assert_matches!(c, '$', "unexpected call to is_delimiter({c:?})");
                false
            })
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, DollarSingleQuote(EscapedString(content)) => {
            assert_eq!(content, []);
        });
    }

    #[test]
    fn lexer_word_unit_dollar_single_quote_nonempty() {
        let mut lexer = Lexer::from_memory(r"$'foo'", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .word_unit(|c| {
                assert_matches!(c, '$', "unexpected call to is_delimiter({c:?})");
                false
            })
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, DollarSingleQuote(EscapedString(content)) => {
            assert_eq!(
                content,
                [
                    EscapeUnit::Literal('f'),
                    EscapeUnit::Literal('o'),
                    EscapeUnit::Literal('o'),
                ]
            );
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_not_dollar_single_quote_in_text_context() {
        let mut lexer = Lexer::from_memory("$''", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Text,
        };
        let result = lexer
            .word_unit(|c| {
                assert_matches!(c, '$', "unexpected call to is_delimiter({c:?})");
                false
            })
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, Unquoted(Literal('$')));
    }

    #[test]
    fn lexer_word_unit_double_quote_empty() {
        let mut lexer = Lexer::from_memory("\"\"", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, DoubleQuote(Text(content)) => assert_eq!(content, []));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_double_quote_non_empty() {
        let mut lexer = Lexer::from_memory("\"abc\"", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, DoubleQuote(Text(content)) => {
            assert_eq!(content, [Literal('a'), Literal('b'), Literal('c')]);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_double_quote_escapes() {
        // Only the following can be escaped in this context: $ ` " \
        let mut lexer = Lexer::from_memory(r#""\a\$\`\"\\\'\#""#, Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .word_unit(|c| match c {
                'a' | '\'' | '#' => true,
                _ => unreachable!("unexpected call to is_delimiter({:?})", c),
            })
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, DoubleQuote(Text(ref units)) => {
            assert_eq!(
                units,
                &[
                    Literal('\\'),
                    Literal('a'),
                    Backslashed('$'),
                    Backslashed('`'),
                    Backslashed('"'),
                    Backslashed('\\'),
                    Literal('\\'),
                    Literal('\''),
                    Literal('\\'),
                    Literal('#'),
                ]
            );
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_double_quote_unclosed() {
        let mut lexer = Lexer::from_memory("\"abc\ndef", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let e = lexer
            .word_unit(|c| unreachable!("unexpected call to is_delimiter({:?})", c))
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedDoubleQuote { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "\"abc\ndef");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(*opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 0..1);
        });
        assert_eq!(*e.location.code.value.borrow(), "\"abc\ndef");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 8..8);
    }

    #[test]
    fn lexer_word_nonempty() {
        let mut lexer = Lexer::from_memory(r"0$(:)X\#", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let word = lexer.word(|_| false).now_or_never().unwrap().unwrap();
        assert_eq!(word.units.len(), 4);
        assert_eq!(word.units[0], WordUnit::Unquoted(Literal('0')));
        assert_matches!(&word.units[1], WordUnit::Unquoted(CommandSubst { content, location }) => {
            assert_eq!(&**content, ":");
            assert_eq!(*location.code.value.borrow(), r"0$(:)X\#");
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(*location.code.source, Source::Unknown);
            assert_eq!(location.range, 1..5);
        });
        assert_eq!(word.units[2], WordUnit::Unquoted(Literal('X')));
        assert_eq!(word.units[3], WordUnit::Unquoted(Backslashed('#')));
        assert_eq!(*word.location.code.value.borrow(), r"0$(:)X\#");
        assert_eq!(word.location.code.start_line_number.get(), 1);
        assert_eq!(*word.location.code.source, Source::Unknown);
        assert_eq!(word.location.range, 0..8);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_empty() {
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let word = lexer
            .word(|_| unreachable!("unexpected call to is_delimiter"))
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(word.units, []);
        assert_eq!(*word.location.code.value.borrow(), "");
        assert_eq!(word.location.code.start_line_number.get(), 1);
        assert_eq!(*word.location.code.source, Source::Unknown);
        assert_eq!(word.location.range, 0..0);
    }

    #[test]
    fn lexer_word_with_switch_in_word_context() {
        let mut lexer = Lexer::from_memory(r"${x-~}", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer
            .word(|c| {
                assert_eq!(c, '\'', "unexpected call to is_delimiter({c:?})");
                false
            })
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_matches!(result.units[..], [Unquoted(BracedParam(ref param))] => {
            assert_matches!(param.modifier, Modifier::Switch(ref switch) => {
                assert_eq!(switch.word.units, [Tilde("".to_string())]);
            });
        });
    }

    #[test]
    fn lexer_word_with_switch_in_text_context() {
        let mut lexer = Lexer::from_memory(r#""${x-~}""#, Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer
            .word(|c| {
                assert_eq!(c, '\'', "unexpected call to is_delimiter({c:?})");
                false
            })
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_matches!(result.units[..], [DoubleQuote(Text(ref units))] => {
            assert_matches!(units[..], [BracedParam(ref param)] => {
                assert_matches!(param.modifier, Modifier::Switch(ref switch) => {
                    assert_eq!(switch.word.units, [Unquoted(Literal('~'))]);
                });
            });
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }
}
