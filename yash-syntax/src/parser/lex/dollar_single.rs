// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Parsing escaped strings

use super::core::Lexer;
use crate::parser::core::Result;
#[cfg(doc)]
use crate::syntax::EscapeUnit;
use crate::syntax::EscapeUnit::*;
use crate::syntax::EscapedString;

impl Lexer<'_> {
    /// Parses an escaped string.
    ///
    /// The `is_delimiter` function is called with each character in the string
    /// to determine if it is a delimiter. If `is_delimiter` returns `true`, the
    /// character is not consumed and the function returns the string up to that
    /// point. Otherwise, the character is consumed and the function continues.
    ///
    /// The string may contain escape sequences as defined in [`EscapeUnit`].
    ///
    /// Escaped strings typically appear as the content of
    /// [dollar-single-quotes], so `is_delimiter` is usually `|c| c == '\''`.
    ///
    /// [dollar-single-quotes]: crate::syntax::WordUnit::DollarSingleQuote
    pub async fn escaped_string<F>(&mut self, mut is_delimiter: F) -> Result<EscapedString>
    where
        F: FnMut(char) -> bool,
    {
        self.escaped_string_dyn(&mut is_delimiter).await
    }

    /// Dynamic version of [`Self::escaped_string`]
    async fn escaped_string_dyn(
        &mut self,
        is_delimiter: &mut dyn FnMut(char) -> bool,
    ) -> Result<EscapedString> {
        let mut units = Vec::new();

        while let Some(c) = self.consume_char_if(|c| !is_delimiter(c)).await? {
            let c = c.value;
            if c == '\\' {
                // TODO Consider extracting this to a separate function
                let Some(c2) = self.peek_char().await? else {
                    todo!("return error");
                };
                self.consume_char();
                match c2 {
                    '"' => units.push(DoubleQuote),
                    '\'' => units.push(SingleQuote),
                    '\\' => units.push(Backslash),
                    '?' => units.push(Question),
                    'a' => units.push(Alert),
                    'b' => units.push(Backspace),
                    'e' | 'E' => units.push(Escape),
                    'f' => units.push(FormFeed),
                    'n' => units.push(Newline),
                    'r' => units.push(CarriageReturn),
                    't' => units.push(Tab),
                    'v' => units.push(VerticalTab),

                    'c' => {
                        let Some(c3) = self.peek_char().await? else {
                            todo!("return error");
                        };
                        self.consume_char();
                        match c3.to_ascii_uppercase() {
                            '\\' => {
                                let Some('\\') = self.peek_char().await? else {
                                    todo!("return error");
                                };
                                self.consume_char();
                                units.push(Control(0x1C));
                            }

                            c3 @ ('\u{3F}'..'\u{60}') => units.push(Control(c3 as u8 ^ 0x40)),

                            _ => todo!("return error: unknown control character {c3:?}"),
                        }
                    }

                    _ => {
                        // Consume at most 3 octal digits (including c2)
                        let Some(mut value) = c2.to_digit(8) else {
                            todo!("return error: unknown escape character {c2:?}");
                        };
                        for _ in 0..2 {
                            let Some(digit) = self.peek_char().await? else {
                                todo!("return error: missing closing quote");
                            };
                            if let Some(digit) = digit.to_digit(8) {
                                value = value * 8 + digit;
                                self.consume_char();
                            } else {
                                break;
                            }
                        }
                        units.push(Octal(value as u8));
                    }
                }
            } else {
                units.push(Literal(c));
            }
        }

        Ok(EscapedString(units))
    }

    /// Parses an escaped string enclosed in single quotes.
    ///
    /// This function is meant to be used for parsing dollar-single-quoted
    /// strings. The initial `$` must have been consumed before calling this
    /// function, which expects an opening `'` to be the next character. If the
    /// next character is not `'`, this function returns `None`.
    ///
    /// This function consumes up to and including the closing `'`. If the
    /// closing `'` is not found or an invalid escape sequence is found, this
    /// function returns an error.
    pub(super) async fn single_quoted_escaped_string(&mut self) -> Result<Option<EscapedString>> {
        let is_single_quote = |c| c == '\'';

        // Consume the opening single quote
        if self.consume_char_if(is_single_quote).await?.is_none() {
            return Ok(None);
        }

        let content = self.escaped_string(is_single_quote).await?;

        // Consume the closing single quote
        if let Some(quote) = self.peek_char().await? {
            debug_assert_eq!(quote, '\'');
            self.consume_char();
            Ok(Some(content))
        } else {
            todo!("return error: missing closing quote");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn escaped_string_literals() {
        let mut lexer = Lexer::from_memory("foo", Source::Unknown);
        let EscapedString(content) = lexer
            .escaped_string(|_| false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(content, [Literal('f'), Literal('o'), Literal('o')]);
    }

    #[test]
    fn escaped_string_named_escapes() {
        let mut lexer = Lexer::from_memory(r#"\""\'\\\?\a\b\e\E\f\n\r\t\v"#, Source::Unknown);
        let EscapedString(content) = lexer
            .escaped_string(|_| false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(
            content,
            [
                DoubleQuote,
                Literal('"'),
                SingleQuote,
                Backslash,
                Question,
                Alert,
                Backspace,
                Escape,
                Escape,
                FormFeed,
                Newline,
                CarriageReturn,
                Tab,
                VerticalTab,
            ]
        );
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    #[ignore = "not implemented"]
    fn escaped_string_incomplete_escapes() {
        todo!()
    }

    #[test]
    fn escaped_string_control_escapes() {
        let mut lexer = Lexer::from_memory(r"\cA\cz\c^\c?\c\\", Source::Unknown);
        let EscapedString(content) = lexer
            .escaped_string(|_| false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(
            content,
            [
                Control(0x01),
                Control(0x1A),
                Control(0x1E),
                Control(0x7F),
                Control(0x1C),
            ]
        );
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    #[ignore = "not implemented"]
    fn single_quoted_escaped_string_incomplete_control_escapes() {
        todo!()
    }

    #[test]
    fn single_quoted_escaped_string_octal_escapes() {
        let mut lexer = Lexer::from_memory(r"\0\07\177\0123", Source::Unknown);
        let EscapedString(content) = lexer
            .escaped_string(|_| false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(
            content,
            [
                Octal(0o0),
                Octal(0o7),
                Octal(0o177),
                Octal(0o12),
                Literal('3'),
            ]
        );
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    #[ignore = "not implemented"]
    fn single_quoted_escaped_string_non_byte_octal_escape() {
        let mut lexer = Lexer::from_memory(r"'\700'", Source::Unknown);
        let result = lexer
            .escaped_string(|_| false)
            .now_or_never()
            .unwrap()
            .unwrap();
        todo!("should be an error: {result:?}");
    }

    #[test]
    #[ignore = "not implemented"]
    fn single_quoted_escaped_string_hex_escapes() {
        todo!()
    }

    #[test]
    #[ignore = "not implemented"]
    fn single_quoted_escaped_string_incomplete_hex_escape() {
        todo!()
    }

    #[test]
    #[ignore = "not implemented"]
    fn single_quoted_escaped_string_unicode_escapes() {
        todo!()
    }

    #[test]
    #[ignore = "not implemented"]
    fn single_quoted_escaped_string_incomplete_unicode_escapes() {
        todo!()
    }

    // TODO Reject non-portable escapes in POSIX mode

    #[test]
    fn single_quoted_escaped_string_empty() {
        let mut lexer = Lexer::from_memory("''", Source::Unknown);
        let result = lexer
            .single_quoted_escaped_string()
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(result, Some(EscapedString(vec![])));
    }

    #[test]
    fn single_quoted_escaped_string_nonempty() {
        let mut lexer = Lexer::from_memory(r"'foo\e'x", Source::Unknown);
        let result = lexer
            .single_quoted_escaped_string()
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_matches!(result, Some(EscapedString(content)) => {
            assert_eq!(
                content,
                [
                    Literal('f'),
                    Literal('o'),
                    Literal('o'),
                    Escape,
                ]
            );
        });
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('x')));
    }

    // TODO single_quoted_escaped_string_unclosed
}
