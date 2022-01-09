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
            Some(c) if c == '\'' && allow_single_quote => {
                let location = self.location().await?.clone();
                self.consume_char();
                self.single_quote(location).await.map(Some)
            }
            Some(c) if c == '"' => {
                let location = self.location().await?.clone();
                self.consume_char();
                self.double_quote(location).await.map(Some)
            }
            _ => Ok(self
                .text_unit(is_delimiter, is_escapable)
                .await?
                .map(Unquoted)),
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
        let location = self.location().await?.clone().into(); // FIXME Correct LocationRef
        let mut units = vec![];
        while let Some(unit) = self.word_unit_dyn(is_delimiter).await? {
            units.push(unit)
        }
        Ok(Word { units, location })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorCause;
    use crate::parser::lex::WordContext;
    use crate::source::Source;
    use crate::syntax::Modifier;
    use crate::syntax::Text;
    use crate::syntax::TextUnit::{self, Backslashed, BracedParam, CommandSubst, Literal};
    use crate::syntax::WordUnit::Tilde;
    use futures_executor::block_on;

    #[test]
    fn lexer_word_unit_unquoted() {
        let mut lexer = Lexer::from_memory("$()", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result =
            block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
                .unwrap()
                .unwrap();
        if let Unquoted(CommandSubst { content, location }) = result {
            assert_eq!(content, "");
            assert_eq!(location.column().get(), 1);
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_unquoted_escapes_in_word_context() {
        // Any characters can be escaped in this context.
        block_on(async {
            let mut lexer = Lexer::from_memory(r#"\a\$\`\"\\\'\#\{\}"#, Source::Unknown);
            let mut lexer = WordLexer {
                lexer: &mut lexer,
                context: WordContext::Word,
            };

            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('a')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('$')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('`')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('"')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('\\')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('\'')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('#')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('{')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('}')))));

            assert_eq!(lexer.peek_char().await, Ok(None));
        })
    }

    #[test]
    fn lexer_word_unit_unquoted_escapes_in_text_context() {
        // $, `, " and \ can be escaped as well as delimiters
        block_on(async {
            let mut lexer = Lexer::from_memory(r#"\a\$\`\"\\\'\#\{\}"#, Source::Unknown);
            let mut lexer = WordLexer {
                lexer: &mut lexer,
                context: WordContext::Text,
            };
            let is_delimiter = |c| c == '}';

            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Literal('\\')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Literal('a')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('$')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('`')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('"')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('\\')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Literal('\\')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Literal('\'')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Literal('\\')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Literal('#')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Literal('\\')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Literal('{')))));
            let result = lexer.word_unit(is_delimiter).await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('}')))));

            assert_eq!(lexer.peek_char().await, Ok(None));
        })
    }

    #[test]
    fn lexer_word_unit_single_quote_empty() {
        let mut lexer = Lexer::from_memory("''", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result =
            block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
                .unwrap()
                .unwrap();
        if let SingleQuote(content) = result {
            assert_eq!(content, "");
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_single_quote_nonempty() {
        let mut lexer = Lexer::from_memory("'abc\\\n$def\\'", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result =
            block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
                .unwrap()
                .unwrap();
        if let SingleQuote(content) = result {
            assert_eq!(content, "abc\\\n$def\\");
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_single_quote_unclosed() {
        let mut lexer = Lexer::from_memory("'abc\ndef\\", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let e = block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
            .unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedSingleQuote { opening_location }) = e.cause {
            assert_eq!(opening_location.code.value, "'abc\n");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("unexpected error cause {:?}", e);
        }
        assert_eq!(e.location.code.value, "def\\");
        assert_eq!(e.location.code.start_line_number.get(), 2);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 5);
    }

    #[test]
    fn lexer_word_unit_not_single_quote_in_text_context() {
        let mut lexer = Lexer::from_memory("'", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Text,
        };

        let result = block_on(lexer.word_unit(|c| {
            assert_eq!(c, '\'', "unexpected call to is_delimiter({:?})", c);
            false
        }))
        .unwrap()
        .unwrap();
        assert_eq!(result, Unquoted(Literal('\'')));

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_double_quote_empty() {
        let mut lexer = Lexer::from_memory("\"\"", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result =
            block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
                .unwrap()
                .unwrap();
        if let DoubleQuote(Text(content)) = result {
            assert_eq!(content, []);
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_double_quote_non_empty() {
        let mut lexer = Lexer::from_memory("\"abc\"", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result =
            block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
                .unwrap()
                .unwrap();
        if let DoubleQuote(Text(content)) = result {
            assert_eq!(content, [Literal('a'), Literal('b'), Literal('c')]);
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_double_quote_escapes() {
        // Only the following can be escaped in this context: $ ` " \
        block_on(async {
            let mut lexer = Lexer::from_memory(r#""\a\$\`\"\\\'\#""#, Source::Unknown);
            let mut lexer = WordLexer {
                lexer: &mut lexer,
                context: WordContext::Word,
            };
            let result = lexer
                .word_unit(|c| match c {
                    'a' | '\'' | '#' => true,
                    _ => panic!("unexpected call to is_delimiter({:?})", c),
                })
                .await
                .unwrap()
                .unwrap();
            if let DoubleQuote(Text(ref units)) = result {
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
            } else {
                panic!("Not a double quote: {:?}", result);
            }

            assert_eq!(lexer.peek_char().await, Ok(None));
        })
    }

    #[test]
    fn lexer_word_unit_double_quote_unclosed() {
        let mut lexer = Lexer::from_memory("\"abc\ndef", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let e = block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
            .unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedDoubleQuote { opening_location }) = e.cause {
            assert_eq!(opening_location.code.value, "\"abc\n");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("unexpected error cause {:?}", e);
        }
        assert_eq!(e.location.code.value, "def");
        assert_eq!(e.location.code.start_line_number.get(), 2);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 4);
    }

    #[test]
    fn lexer_word_nonempty() {
        let mut lexer = Lexer::from_memory(r"0$(:)X\#", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let word = block_on(lexer.word(|_| false)).unwrap();
        assert_eq!(word.units.len(), 4);
        assert_eq!(word.units[0], WordUnit::Unquoted(TextUnit::Literal('0')));
        if let WordUnit::Unquoted(TextUnit::CommandSubst { content, location }) = &word.units[1] {
            assert_eq!(content, ":");
            assert_eq!(location.code().value, r"0$(:)X\#");
            assert_eq!(location.code().start_line_number.get(), 1);
            assert_eq!(location.code().source, Source::Unknown);
            assert_eq!(location.column().get(), 2);
        } else {
            panic!("unexpected word unit: {:?}", word.units[1]);
        }
        assert_eq!(word.units[2], WordUnit::Unquoted(TextUnit::Literal('X')));
        assert_eq!(
            word.units[3],
            WordUnit::Unquoted(TextUnit::Backslashed('#'))
        );
        assert_eq!(word.location.code().value, r"0$(:)X\#");
        assert_eq!(word.location.code().start_line_number.get(), 1);
        assert_eq!(word.location.code().source, Source::Unknown);
        assert_eq!(word.location.column().get(), 1);

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_empty() {
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let word = block_on(lexer.word(|_| panic!("unexpected call to is_delimiter"))).unwrap();
        assert_eq!(word.units, []);
        assert_eq!(word.location.code().value, "");
        assert_eq!(word.location.code().start_line_number.get(), 1);
        assert_eq!(word.location.code().source, Source::Unknown);
        assert_eq!(word.location.column().get(), 1);
    }

    #[test]
    fn lexer_word_with_switch_in_word_context() {
        let mut lexer = Lexer::from_memory(r"${x-~}", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.word(|c| {
            assert_eq!(c, '\'', "unexpected call to is_delimiter({:?})", c);
            false
        }))
        .unwrap();
        if let [Unquoted(BracedParam(ref param))] = result.units[..] {
            if let Modifier::Switch(ref switch) = param.modifier {
                assert_eq!(switch.word.units, [Tilde("".to_string())]);
            } else {
                panic!("Not a switch: {:?}", param.modifier);
            }
        } else {
            panic!("Not a single parameter: {:?}", result.units);
        }
    }

    #[test]
    fn lexer_word_with_switch_in_text_context() {
        let mut lexer = Lexer::from_memory(r#""${x-~}""#, Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.word(|c| {
            assert_eq!(c, '\'', "unexpected call to is_delimiter({:?})", c);
            false
        }))
        .unwrap();
        if let [DoubleQuote(Text(ref units))] = result.units[..] {
            if let [BracedParam(ref param)] = units[..] {
                if let Modifier::Switch(ref switch) = param.modifier {
                    assert_eq!(switch.word.units, [Unquoted(Literal('~'))]);
                } else {
                    panic!("Not a switch: {:?}", param.modifier);
                }
            } else {
                panic!("Not a single parameter: {:?}", units);
            }
        } else {
            panic!("Not a single double-quote: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }
}
