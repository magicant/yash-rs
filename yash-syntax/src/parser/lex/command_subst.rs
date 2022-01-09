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
use crate::source::LocationRef;
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
    /// `opening_location` should be the location of the initial `$`. It is used
    /// to construct the result, but this function does not check if it actually
    /// is a location of `$`.
    pub async fn command_substitution(
        &mut self,
        opening_location: LocationRef,
    ) -> Result<Option<TextUnit>> {
        if !self.skip_if(|c| c == '(').await? {
            return Ok(None);
        }

        let content = self.inner_program_boxed().await?;

        if !self.skip_if(|c| c == ')').await? {
            // TODO Return a better error depending on the token id of the next token
            let opening_location = opening_location.get();
            let cause = SyntaxError::UnclosedCommandSubstitution { opening_location }.into();
            let location = self.location().await?.clone();
            return Err(Error { cause, location });
        }

        let location = opening_location;
        Ok(Some(TextUnit::CommandSubst { content, location }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorCause;
    use crate::source::LocationRef;
    use crate::source::Source;
    use futures_executor::block_on;

    #[test]
    fn lexer_command_substitution_success() {
        let mut lexer = Lexer::from_memory("( foo bar )baz", Source::Unknown);
        let location = LocationRef::dummy("X");

        let result = block_on(lexer.command_substitution(location))
            .unwrap()
            .unwrap();
        if let TextUnit::CommandSubst { location, content } = result {
            assert_eq!(location.code().value, "X");
            assert_eq!(location.code().start_line_number.get(), 1);
            assert_eq!(location.code().source, Source::Unknown);
            assert_eq!(location.column().get(), 1);
            assert_eq!(content, " foo bar ");
        } else {
            panic!("unexpected result {:?}", result);
        }

        let next = block_on(lexer.location()).unwrap();
        assert_eq!(next.code.value, "( foo bar )baz");
        assert_eq!(next.code.start_line_number.get(), 1);
        assert_eq!(next.code.source, Source::Unknown);
        assert_eq!(next.column.get(), 12);
    }

    #[test]
    fn lexer_command_substitution_none() {
        let mut lexer = Lexer::from_memory(" foo bar )baz", Source::Unknown);
        let location = LocationRef::dummy("Y");

        let result = block_on(lexer.command_substitution(location)).unwrap();
        assert_eq!(result, None);

        let next = block_on(lexer.location()).unwrap();
        assert_eq!(next.code.value, " foo bar )baz");
        assert_eq!(next.code.start_line_number.get(), 1);
        assert_eq!(next.code.source, Source::Unknown);
        assert_eq!(next.column.get(), 1);
    }

    #[test]
    fn lexer_command_substitution_unclosed() {
        let mut lexer = Lexer::from_memory("( foo bar baz", Source::Unknown);
        let location = LocationRef::dummy("Z");

        let e = block_on(lexer.command_substitution(location)).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedCommandSubstitution { opening_location }) =
            e.cause
        {
            assert_eq!(opening_location.code.value, "Z");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("unexpected error cause {:?}", e);
        }
        assert_eq!(e.location.code.value, "( foo bar baz");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 14);
    }
}
