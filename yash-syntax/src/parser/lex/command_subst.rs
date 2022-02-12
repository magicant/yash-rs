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

//! Part of the lexer that parses command substitutions.

use super::core::Lexer;
use crate::parser::core::Result;
use crate::parser::error::Error;
use crate::parser::error::SyntaxError;
use crate::source::Location;
use crate::source::Span;
use crate::syntax::TextUnit;

impl Lexer<'_> {
    /// Parses a command substitution of the form `$(...)`.
    ///
    /// The initial `$` must have been consumed before calling this function.
    /// In this function, the next character is examined to see if it begins a
    /// command substitution. If it is `(`, the following characters are parsed
    /// as commands to find a matching `)`, which will be consumed before this
    /// function returns. Otherwise, no characters are consumed and the return
    /// value is `Ok(None)`.
    ///
    /// The `start` parameter should be the index for the initial `$`. It is
    /// used to construct the result, but this function does not check if it
    /// actually points to the `$`.
    pub async fn command_substitution(&mut self, start: usize) -> Result<Option<TextUnit>> {
        if !self.skip_if(|c| c == '(').await? {
            return Ok(None);
        }

        let content = self.inner_program_boxed().await?;

        if !self.skip_if(|c| c == ')').await? {
            // TODO Return a better error depending on the token id of the next token
            let Span { code, range } = self.span(start..start);
            let opening_location = Location {
                code,
                index: range.start,
            };
            let cause = SyntaxError::UnclosedCommandSubstitution { opening_location }.into();
            let location = self.location().await?.clone();
            return Err(Error { cause, location });
        }

        let span = self.span(start..self.index());
        Ok(Some(TextUnit::CommandSubst { content, span }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorCause;
    use crate::source::Source;
    use assert_matches::assert_matches;
    use futures_executor::block_on;

    #[test]
    fn lexer_command_substitution_success() {
        let mut lexer = Lexer::from_memory("$( foo bar )baz", Source::Unknown);
        block_on(lexer.peek_char()).unwrap();
        lexer.consume_char();

        let result = block_on(lexer.command_substitution(0)).unwrap().unwrap();
        assert_matches!(result, TextUnit::CommandSubst { content, span } => {
            assert_eq!(*span.code.value.borrow(), "$( foo bar )baz");
            assert_eq!(span.code.start_line_number.get(), 1);
            assert_eq!(span.code.source, Source::Unknown);
            assert_eq!(span.range, 0..12);
            assert_eq!(content, " foo bar ");
        });

        let next = block_on(lexer.location()).unwrap();
        assert_eq!(*next.code.value.borrow(), "$( foo bar )baz");
        assert_eq!(next.code.start_line_number.get(), 1);
        assert_eq!(next.code.source, Source::Unknown);
        assert_eq!(next.index, 12);
    }

    #[test]
    fn lexer_command_substitution_none() {
        let mut lexer = Lexer::from_memory("$ foo bar )baz", Source::Unknown);
        block_on(lexer.peek_char()).unwrap();
        lexer.consume_char();

        let result = block_on(lexer.command_substitution(0)).unwrap();
        assert_eq!(result, None);

        let next = block_on(lexer.location()).unwrap();
        assert_eq!(*next.code.value.borrow(), "$ foo bar )baz");
        assert_eq!(next.code.start_line_number.get(), 1);
        assert_eq!(next.code.source, Source::Unknown);
        assert_eq!(next.index, 1);
    }

    #[test]
    fn lexer_command_substitution_unclosed() {
        let mut lexer = Lexer::from_memory("$( foo bar baz", Source::Unknown);
        block_on(lexer.peek_char()).unwrap();
        lexer.consume_char();

        let e = block_on(lexer.command_substitution(0)).unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedCommandSubstitution { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "$( foo bar baz");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.index, 0);
        });
        assert_eq!(*e.location.code.value.borrow(), "$( foo bar baz");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 14);
    }
}
