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

//! Part of the lexer that parses raw parameter expansion.

use super::core::Lexer;
use crate::parser::core::Result;
use crate::syntax::TextUnit;

/// Tests if a character can be part of a POSIXly-portable name.
///
/// Returns true if the character is an ASCII alphanumeric or underscore.
///
/// Note that a valid name cannot start with a digit.
pub fn is_portable_name_char(c: char) -> bool {
    matches!(c, '0'..='9' | 'A'..='Z' | '_' | 'a'..='z')
}

/// Tests if a character names a special parameter.
///
/// A special parameter is one of: `@*#?-$!0`.
pub fn is_special_parameter_char(c: char) -> bool {
    matches!(c, '@' | '*' | '#' | '?' | '-' | '$' | '!' | '0')
}

/// Tests if a character is a valid single-character raw parameter name.
///
/// If this function returns true, the character is a valid parameter name for a
/// raw parameter expansion, but the next character is never treated as part of
/// the name.
///
/// This function returns true for ASCII digits and special parameter names.
pub fn is_single_char_name(c: char) -> bool {
    c.is_ascii_digit() || is_special_parameter_char(c)
}

impl Lexer<'_> {
    /// Parses a parameter expansion that is not enclosed in braces.
    ///
    /// The initial `$` must have been consumed before calling this function.
    /// This functions checks if the next character is a valid POSIXly-portable
    /// parameter name. If so, the name is consumed and returned. Otherwise, no
    /// characters are consumed and the return value is `Ok(None)`.
    ///
    /// The `start` parameter should be the index for the initial `$`. It is
    /// used to construct the result, but this function does not check if it
    /// actually points to the `$`.
    pub async fn raw_param(&mut self, start: usize) -> Result<Option<TextUnit>> {
        if let Some(c) = self.consume_char_if(is_single_char_name).await? {
            let name = c.value.to_string();
            let span = self.span(start..self.index());
            Ok(Some(TextUnit::RawParam { name, span }))
        } else if let Some(c) = self.consume_char_if(is_portable_name_char).await? {
            let mut name = c.value.to_string();
            while let Some(c) = self.consume_char_if(is_portable_name_char).await? {
                name.push(c.value);
            }
            let span = self.span(start..self.index());
            Ok(Some(TextUnit::RawParam { name, span }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use assert_matches::assert_matches;
    use futures_executor::block_on;

    #[test]
    fn lexer_raw_param_special_parameter() {
        block_on(async {
            let mut lexer = Lexer::from_memory("$@;", Source::Unknown);
            lexer.peek_char().await.unwrap();
            lexer.consume_char();

            let result = lexer.raw_param(0).await.unwrap().unwrap();
            assert_matches!(result, TextUnit::RawParam { name, span } => {
                assert_eq!(name, "@");
                assert_eq!(*span.code.value.borrow(), "$@;");
                assert_eq!(span.code.start_line_number.get(), 1);
                assert_eq!(span.code.source, Source::Unknown);
                assert_eq!(span.range, 0..2);
            });

            assert_eq!(lexer.peek_char().await, Ok(Some(';')));
        })
    }

    #[test]
    fn lexer_raw_param_digit() {
        block_on(async {
            let mut lexer = Lexer::from_memory("$12", Source::Unknown);
            lexer.peek_char().await.unwrap();
            lexer.consume_char();

            let result = lexer.raw_param(0).await.unwrap().unwrap();
            assert_matches!(result, TextUnit::RawParam { name, span } => {
                assert_eq!(name, "1");
                assert_eq!(*span.code.value.borrow(), "$12");
                assert_eq!(span.code.start_line_number.get(), 1);
                assert_eq!(span.code.source, Source::Unknown);
                assert_eq!(span.range, 0..2);
            });

            assert_eq!(lexer.peek_char().await, Ok(Some('2')));
        })
    }

    #[test]
    fn lexer_raw_param_posix_name() {
        block_on(async {
            let mut lexer = Lexer::from_memory("$az_AZ_019<", Source::Unknown);
            lexer.peek_char().await.unwrap();
            lexer.consume_char();

            let result = lexer.raw_param(0).await.unwrap().unwrap();
            assert_matches!(result, TextUnit::RawParam { name, span } => {
                assert_eq!(name, "az_AZ_019");
                assert_eq!(*span.code.value.borrow(), "$az_AZ_019<");
                assert_eq!(span.code.start_line_number.get(), 1);
                assert_eq!(span.code.source, Source::Unknown);
                assert_eq!(span.range, 0..10);
            });

            assert_eq!(lexer.peek_char().await, Ok(Some('<')));
        })
    }

    #[test]
    fn lexer_raw_param_posix_name_line_continuations() {
        block_on(async {
            let mut lexer = Lexer::from_memory("$a\\\n\\\nb\\\n\\\nc\\\n>", Source::Unknown);
            lexer.peek_char().await.unwrap();
            lexer.consume_char();

            let result = lexer.raw_param(0).await.unwrap().unwrap();
            assert_matches!(result, TextUnit::RawParam { name, span } => {
                assert_eq!(name, "abc");
                assert_eq!(*span.code.value.borrow(), "$a\\\n\\\nb\\\n\\\nc\\\n>");
                assert_eq!(span.code.start_line_number.get(), 1);
                assert_eq!(span.code.source, Source::Unknown);
                assert_eq!(span.range, 0..14);
            });

            assert_eq!(lexer.peek_char().await, Ok(Some('>')));
        })
    }

    #[test]
    fn lexer_raw_param_not_parameter() {
        let mut lexer = Lexer::from_memory(";", Source::Unknown);
        assert_eq!(block_on(lexer.raw_param(0)), Ok(None));
        assert_eq!(block_on(lexer.peek_char()), Ok(Some(';')));
    }
}
