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

//! Part of the lexer that parses arithmetic expansions.

use super::core::Lexer;
use crate::parser::core::Result;
use crate::parser::error::Error;
use crate::parser::error::SyntaxError;
use crate::source::Location;
use crate::source::Span;
use crate::syntax::Text;
use crate::syntax::TextUnit;
use std::future::Future;
use std::pin::Pin;

impl Lexer<'_> {
    /// Parses an arithmetic expansion.
    ///
    /// The initial `$` must have been consumed before calling this function.
    /// In this function, the next two characters are examined to see if they
    /// begin an arithmetic expansion. If the characters are `((`, then the
    /// arithmetic expansion is parsed, in which case this function consumes up
    /// to the closing `))` (inclusive). Otherwise, no characters are consumed
    /// and the return value is `Ok(None)`.
    ///
    /// The `start` parameter should be the index for the initial `$`. It is
    /// used to construct the result, but this function does not check if it
    /// actually points to the `$`.
    pub async fn arithmetic_expansion(&mut self, start: usize) -> Result<Option<TextUnit>> {
        let index = self.index();

        // Part 1: Parse `((`
        if !self.skip_if(|c| c == '(').await? {
            return Ok(None);
        }
        if !self.skip_if(|c| c == '(').await? {
            self.rewind(index);
            return Ok(None);
        }

        // Part 2: Parse the content
        let is_delimiter = |c| c == ')';
        let is_escapable = |c| matches!(c, '$' | '`' | '\\');
        // Boxing needed for recursion
        let content: Pin<Box<dyn Future<Output = Result<Text>>>> =
            Box::pin(self.text_with_parentheses(is_delimiter, is_escapable));
        let content = content.await?;

        // Part 3: Parse `))`
        match self.peek_char().await? {
            Some(sc) if sc == ')' => self.consume_char(),
            Some(_) => unreachable!(),
            None => {
                let Span { code, range } = self.span(start..start);
                let opening_location = Location {
                    code,
                    index: range.start,
                };
                let cause = SyntaxError::UnclosedArith { opening_location }.into();
                let location = self.location().await?.clone();
                return Err(Error { cause, location });
            }
        }
        match self.peek_char().await? {
            Some(sc) if sc == ')' => self.consume_char(),
            Some(_) => {
                self.rewind(index);
                return Ok(None);
            }
            None => {
                let Span { code, range } = self.span(start..start);
                let opening_location = Location {
                    code,
                    index: range.start,
                };
                let cause = SyntaxError::UnclosedArith { opening_location }.into();
                let location = self.location().await?.clone();
                return Err(Error { cause, location });
            }
        }

        let span = self.span(start..self.index());
        Ok(Some(TextUnit::Arith { content, span }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorCause;
    use crate::source::Source;
    use crate::syntax::Backslashed;
    use crate::syntax::Literal;
    use assert_matches::assert_matches;
    use futures_executor::block_on;

    #[test]
    fn lexer_arithmetic_expansion_empty() {
        let mut lexer = Lexer::from_memory("$(());", Source::Unknown);
        block_on(lexer.peek_char()).unwrap();
        lexer.consume_char();

        let result = block_on(lexer.arithmetic_expansion(0)).unwrap().unwrap();
        assert_matches!(result, TextUnit::Arith { content, span } => {
            assert_eq!(content.0, []);
            assert_eq!(*span.code.value.borrow(), "$(());");
            assert_eq!(span.code.start_line_number.get(), 1);
            assert_eq!(span.code.source, Source::Unknown);
            assert_eq!(span.range, 0..5);
        });

        assert_eq!(block_on(lexer.peek_char()), Ok(Some(';')));
    }

    #[test]
    fn lexer_arithmetic_expansion_none() {
        let mut lexer = Lexer::from_memory("( foo bar )baz", Source::Unknown);
        assert_eq!(block_on(lexer.arithmetic_expansion(0)), Ok(None));
        assert_eq!(block_on(lexer.peek_char()), Ok(Some('(')));
    }

    #[test]
    fn lexer_arithmetic_expansion_line_continuations() {
        let mut lexer = Lexer::from_memory("$(\\\n\\\n(\\\n)\\\n\\\n);", Source::Unknown);
        block_on(lexer.peek_char()).unwrap();
        lexer.consume_char();

        let result = block_on(lexer.arithmetic_expansion(0)).unwrap().unwrap();
        assert_matches!(result, TextUnit::Arith { content, span } => {
            assert_eq!(content.0, []);
            assert_eq!(*span.code.value.borrow(), "$(\\\n\\\n(\\\n)\\\n\\\n);");
            assert_eq!(span.code.start_line_number.get(), 1);
            assert_eq!(span.code.source, Source::Unknown);
            assert_eq!(span.range, 0..15);
        });

        assert_eq!(block_on(lexer.peek_char()), Ok(Some(';')));
    }

    #[test]
    fn lexer_arithmetic_expansion_escapes() {
        let mut lexer = Lexer::from_memory(r#"$((\\\"\`\$));"#, Source::Unknown);
        block_on(lexer.peek_char()).unwrap();
        lexer.consume_char();

        let result = block_on(lexer.arithmetic_expansion(0)).unwrap().unwrap();
        assert_matches!(result, TextUnit::Arith { content, span } => {
            assert_eq!(
                content.0,
                [
                    Backslashed('\\'),
                    Literal('\\'),
                    Literal('"'),
                    Backslashed('`'),
                    Backslashed('$')
                ]
            );
            assert_eq!(*span.code.value.borrow(), r#"$((\\\"\`\$));"#);
            assert_eq!(span.code.start_line_number.get(), 1);
            assert_eq!(span.code.source, Source::Unknown);
            assert_eq!(span.range, 0..13);
        });

        assert_eq!(block_on(lexer.peek_char()), Ok(Some(';')));
    }

    #[test]
    fn lexer_arithmetic_expansion_unclosed_first() {
        let mut lexer = Lexer::from_memory("$((1", Source::Unknown);
        block_on(lexer.peek_char()).unwrap();
        lexer.consume_char();

        let e = block_on(lexer.arithmetic_expansion(0)).unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedArith { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "$((1");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.index, 0);
        });
        assert_eq!(*e.location.code.value.borrow(), "$((1");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 4);
    }

    #[test]
    fn lexer_arithmetic_expansion_unclosed_second() {
        let mut lexer = Lexer::from_memory("$((1)", Source::Unknown);
        block_on(lexer.peek_char()).unwrap();
        lexer.consume_char();

        let e = block_on(lexer.arithmetic_expansion(0)).unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedArith { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "$((1)");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.index, 0);
        });
        assert_eq!(*e.location.code.value.borrow(), "$((1)");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 5);
    }

    #[test]
    fn lexer_arithmetic_expansion_unclosed_but_maybe_command_substitution() {
        let mut lexer = Lexer::from_memory("((1) ", Source::Unknown);
        assert_eq!(block_on(lexer.arithmetic_expansion(0)), Ok(None));
        assert_eq!(lexer.index(), 0);
    }
}
