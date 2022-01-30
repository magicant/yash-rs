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

//! Part of the lexer that parses backquotes.

use super::core::WordContext;
use super::core::WordLexer;
use crate::parser::core::Result;
use crate::parser::error::Error;
use crate::parser::error::SyntaxError;
use crate::syntax::BackquoteUnit;
use crate::syntax::TextUnit;

impl WordLexer<'_, '_> {
    /// Parses a backquote unit.
    async fn backquote_unit(&mut self) -> Result<Option<BackquoteUnit>> {
        if self.skip_if(|c| c == '\\').await? {
            let double_quote_escapable = match self.context {
                WordContext::Word => false,
                WordContext::Text => true,
            };
            let is_escapable =
                |c| matches!(c, '$' | '`' | '\\') || c == '"' && double_quote_escapable;
            if let Some(c) = self.consume_char_if(is_escapable).await? {
                return Ok(Some(BackquoteUnit::Backslashed(c.value)));
            } else {
                return Ok(Some(BackquoteUnit::Literal('\\')));
            }
        }

        if let Some(c) = self.consume_char_if(|c| c != '`').await? {
            return Ok(Some(BackquoteUnit::Literal(c.value)));
        }

        Ok(None)
    }

    /// Parses a command substitution of the form `` `...` ``.
    ///
    /// If the next character is a backquote, the command substitution is parsed
    /// up to the closing backquote (inclusive). It is a syntax error if there is
    /// no closing backquote.
    ///
    /// Between the backquotes, only backslashes can have special meanings. A
    /// backslash is an escape character if it precedes a dollar, backquote, or
    /// another backslash. If `self.context` is `Text`, double quotes can also
    /// be backslash-escaped.
    pub async fn backquote(&mut self) -> Result<Option<TextUnit>> {
        let location = match self.consume_char_if(|c| c == '`').await? {
            None => return Ok(None),
            Some(c) => c.location.clone(),
        };

        let mut content = Vec::new();
        while let Some(unit) = self.backquote_unit().await? {
            content.push(unit);
        }

        if self.skip_if(|c| c == '`').await? {
            Ok(Some(TextUnit::Backquote { content, location }))
        } else {
            let opening_location = location;
            let cause = SyntaxError::UnclosedBackquote { opening_location }.into();
            let location = self.location().await?.clone();
            Err(Error { cause, location })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorCause;
    use crate::parser::lex::Lexer;
    use crate::source::Source;
    use assert_matches::assert_matches;
    use futures_executor::block_on;

    #[test]
    fn lexer_backquote_not_backquote() {
        let mut lexer = Lexer::from_memory("X", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.backquote()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn lexer_backquote_empty() {
        let mut lexer = Lexer::from_memory("``", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.backquote()).unwrap().unwrap();
        assert_matches!(result, TextUnit::Backquote { content, location } => {
            assert_eq!(content, []);
            assert_eq!(location.index, 0);
        });

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_backquote_literals() {
        let mut lexer = Lexer::from_memory("`echo`", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.backquote()).unwrap().unwrap();
        assert_matches!(result, TextUnit::Backquote { content, location } => {
            assert_eq!(
                content,
                [
                    BackquoteUnit::Literal('e'),
                    BackquoteUnit::Literal('c'),
                    BackquoteUnit::Literal('h'),
                    BackquoteUnit::Literal('o')
                ]
            );
            assert_eq!(location.index, 0);
        });

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_backquote_with_escapes_double_quote_escapable() {
        let mut lexer = Lexer::from_memory(r#"`a\a\$\`\\\"\'`"#, Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Text,
        };
        let result = block_on(lexer.backquote()).unwrap().unwrap();
        assert_matches!(result, TextUnit::Backquote { content, location } => {
            assert_eq!(
                content,
                [
                    BackquoteUnit::Literal('a'),
                    BackquoteUnit::Literal('\\'),
                    BackquoteUnit::Literal('a'),
                    BackquoteUnit::Backslashed('$'),
                    BackquoteUnit::Backslashed('`'),
                    BackquoteUnit::Backslashed('\\'),
                    BackquoteUnit::Backslashed('"'),
                    BackquoteUnit::Literal('\\'),
                    BackquoteUnit::Literal('\'')
                ]
            );
            assert_eq!(location.index, 0);
        });

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_backquote_with_escapes_double_quote_not_escapable() {
        let mut lexer = Lexer::from_memory(r#"`a\a\$\`\\\"\'`"#, Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.backquote()).unwrap().unwrap();
        assert_matches!(result, TextUnit::Backquote { content, location } => {
            assert_eq!(
                content,
                [
                    BackquoteUnit::Literal('a'),
                    BackquoteUnit::Literal('\\'),
                    BackquoteUnit::Literal('a'),
                    BackquoteUnit::Backslashed('$'),
                    BackquoteUnit::Backslashed('`'),
                    BackquoteUnit::Backslashed('\\'),
                    BackquoteUnit::Literal('\\'),
                    BackquoteUnit::Literal('"'),
                    BackquoteUnit::Literal('\\'),
                    BackquoteUnit::Literal('\'')
                ]
            );
            assert_eq!(location.index, 0);
        });

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_backquote_line_continuation() {
        let mut lexer = Lexer::from_memory("`\\\na\\\n\\\nb\\\n`", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.backquote()).unwrap().unwrap();
        assert_matches!(result, TextUnit::Backquote { content, location } => {
            assert_eq!(
                content,
                [BackquoteUnit::Literal('a'), BackquoteUnit::Literal('b')]
            );
            assert_eq!(location.index, 0);
        });

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_backquote_unclosed_empty() {
        let mut lexer = Lexer::from_memory("`", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let e = block_on(lexer.backquote()).unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedBackquote { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "`");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.index, 0);
        });
        assert_eq!(*e.location.code.value.borrow(), "`");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 1);
    }

    #[test]
    fn lexer_backquote_unclosed_nonempty() {
        let mut lexer = Lexer::from_memory("`foo", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let e = block_on(lexer.backquote()).unwrap_err();
        assert_matches!(e.cause, ErrorCause::Syntax(SyntaxError::UnclosedBackquote { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "`foo");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.index, 0);
        });
        assert_eq!(*e.location.code.value.borrow(), "`foo");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 4);
    }
}
