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
use crate::syntax::TextUnit;

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
    /// The `start_index` parameter should be the index for the initial `$`. It is
    /// used to construct the result, but this function does not check if it
    /// actually points to the `$`.
    pub async fn arithmetic_expansion(&mut self, start_index: usize) -> Result<Option<TextUnit>> {
        let orig_index = self.index();

        // Part 1: Parse `((`
        if !self.skip_if(|c| c == '(').await? {
            return Ok(None);
        }
        if !self.skip_if(|c| c == '(').await? {
            self.rewind(orig_index);
            return Ok(None);
        }

        let opening_location = self.location_range(start_index..self.index());

        // Part 2: Parse the content
        let is_delimiter = |c| c == ')';
        let is_escapable = |c| matches!(c, '$' | '`' | '\\');
        // Boxing needed for recursion
        let content = Box::pin(self.text_with_parentheses(is_delimiter, is_escapable)).await?;

        // Part 3: Parse `))`
        match self.peek_char().await? {
            Some(')') => self.consume_char(),
            Some(_) => unreachable!(),
            None => {
                let cause = SyntaxError::UnclosedArith { opening_location }.into();
                let location = self.location().await?.clone();
                return Err(Error { cause, location });
            }
        }
        match self.peek_char().await? {
            Some(')') => self.consume_char(),
            Some(_) => {
                self.rewind(orig_index);
                return Ok(None);
            }
            None => {
                let cause = SyntaxError::UnclosedArith { opening_location }.into();
                let location = self.location().await?.clone();
                return Err(Error { cause, location });
            }
        }

        let location = self.location_range(start_index..self.index());
        Ok(Some(TextUnit::Arith { content, location }))
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
    use futures_util::FutureExt;

    #[test]
    fn lexer_arithmetic_expansion_empty() {
        let mut lexer = Lexer::from_memory("$(());", Source::Unknown);
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.arithmetic_expansion(0).now_or_never().unwrap();
        let text_unit = result.unwrap().unwrap();
        assert_matches!(text_unit, TextUnit::Arith { content, location } => {
            assert_eq!(content.0, []);
            assert_eq!(*location.code.value.borrow(), "$(());");
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(location.code.source, Source::Unknown);
            assert_eq!(location.range, 0..5);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(';')));
    }

    #[test]
    fn lexer_arithmetic_expansion_none() {
        let mut lexer = Lexer::from_memory("$( foo bar )baz", Source::Unknown);
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();
        assert_eq!(
            lexer.arithmetic_expansion(0).now_or_never().unwrap(),
            Ok(None)
        );
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('(')));
    }

    #[test]
    fn lexer_arithmetic_expansion_line_continuations() {
        let mut lexer = Lexer::from_memory("$(\\\n\\\n(\\\n)\\\n\\\n);", Source::Unknown);
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.arithmetic_expansion(0).now_or_never().unwrap();
        let text_unit = result.unwrap().unwrap();
        assert_matches!(text_unit, TextUnit::Arith { content, location } => {
            assert_eq!(content.0, []);
            assert_eq!(*location.code.value.borrow(), "$(\\\n\\\n(\\\n)\\\n\\\n);");
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(location.code.source, Source::Unknown);
            assert_eq!(location.range, 0..15);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(';')));
    }

    #[test]
    fn lexer_arithmetic_expansion_escapes() {
        let mut lexer = Lexer::from_memory(r#".$((\\\"\`\$));"#, Source::Unknown);
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.arithmetic_expansion(1).now_or_never().unwrap();
        let text_unit = result.unwrap().unwrap();
        assert_matches!(text_unit, TextUnit::Arith { content, location } => {
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
            assert_eq!(*location.code.value.borrow(), r#".$((\\\"\`\$));"#);
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(location.code.source, Source::Unknown);
            assert_eq!(location.range, 1..14);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(';')));
    }

    #[test]
    fn lexer_arithmetic_expansion_unclosed_first() {
        let mut lexer = Lexer::from_memory("$((1", Source::Unknown);
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.arithmetic_expansion(0).now_or_never().unwrap();
        let e = result.unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedArith { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "$((1");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 0..3);
        });
        assert_eq!(*e.location.code.value.borrow(), "$((1");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 4..4);
    }

    #[test]
    fn lexer_arithmetic_expansion_unclosed_second() {
        let mut lexer = Lexer::from_memory("$((1)", Source::Unknown);
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.arithmetic_expansion(0).now_or_never().unwrap();
        let e = result.unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedArith { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "$((1)");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 0..3);
        });
        assert_eq!(*e.location.code.value.borrow(), "$((1)");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 5..5);
    }

    #[test]
    fn lexer_arithmetic_expansion_unclosed_but_maybe_command_substitution() {
        let mut lexer = Lexer::from_memory("$((1) ", Source::Unknown);
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();
        assert_eq!(
            lexer.arithmetic_expansion(0).now_or_never().unwrap(),
            Ok(None)
        );
        assert_eq!(lexer.index(), 1);
    }
}
