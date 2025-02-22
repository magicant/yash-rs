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

//! Part of the lexer that parses backquotes

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
            match self.consume_char_if(is_escapable).await? { Some(c) => {
                return Ok(Some(BackquoteUnit::Backslashed(c.value)));
            } _ => {
                return Ok(Some(BackquoteUnit::Literal('\\')));
            }}
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
        let start = self.index();
        let opening_location = match self.consume_char_if(|c| c == '`').await? {
            None => return Ok(None),
            Some(c) => c.location.clone(),
        };

        let mut content = Vec::new();
        while let Some(unit) = self.backquote_unit().await? {
            content.push(unit);
        }

        if self.skip_if(|c| c == '`').await? {
            let location = self.location_range(start..self.index());
            Ok(Some(TextUnit::Backquote { content, location }))
        } else {
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
    use futures_util::FutureExt;

    #[test]
    fn lexer_backquote_not_backquote() {
        let mut lexer = Lexer::with_code("X");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer.backquote().now_or_never().unwrap().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn lexer_backquote_empty() {
        let mut lexer = Lexer::with_code("``");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer.backquote().now_or_never().unwrap().unwrap().unwrap();
        assert_matches!(result, TextUnit::Backquote { content, location } => {
            assert_eq!(content, []);
            assert_eq!(location.range, 0..2);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_backquote_literals() {
        let mut lexer = Lexer::with_code("`echo`");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer.backquote().now_or_never().unwrap().unwrap().unwrap();
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
            assert_eq!(location.range, 0..6);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_backquote_with_escapes_double_quote_escapable() {
        let mut lexer = Lexer::with_code(r#"`a\a\$\`\\\"\'`"#);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Text,
        };
        let result = lexer.backquote().now_or_never().unwrap().unwrap().unwrap();
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
            assert_eq!(location.range, 0..15);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_backquote_with_escapes_double_quote_not_escapable() {
        let mut lexer = Lexer::with_code(r#"`a\a\$\`\\\"\'`"#);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer.backquote().now_or_never().unwrap().unwrap().unwrap();
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
            assert_eq!(location.range, 0..15);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_backquote_line_continuation() {
        let mut lexer = Lexer::with_code("`\\\na\\\n\\\nb\\\n`");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer.backquote().now_or_never().unwrap().unwrap().unwrap();
        assert_matches!(result, TextUnit::Backquote { content, location } => {
            assert_eq!(
                content,
                [BackquoteUnit::Literal('a'), BackquoteUnit::Literal('b')]
            );
            assert_eq!(location.range, 0..12);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_backquote_unclosed_empty() {
        let mut lexer = Lexer::with_code("`");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let e = lexer.backquote().now_or_never().unwrap().unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedBackquote { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "`");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(*opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 0..1);
        });
        assert_eq!(*e.location.code.value.borrow(), "`");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 1..1);
    }

    #[test]
    fn lexer_backquote_unclosed_nonempty() {
        let mut lexer = Lexer::with_code("`foo");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let e = lexer.backquote().now_or_never().unwrap().unwrap_err();
        assert_matches!(e.cause, ErrorCause::Syntax(SyntaxError::UnclosedBackquote { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "`foo");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(*opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 0..1);
        });
        assert_eq!(*e.location.code.value.borrow(), "`foo");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 4..4);
    }
}
