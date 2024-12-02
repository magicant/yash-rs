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

//! Parsing escape units and escaped strings

use super::core::Lexer;
use crate::parser::core::Result;
use crate::parser::{Error, SyntaxError};
use crate::syntax::EscapeUnit::{self, *};
use crate::syntax::EscapedString;

impl Lexer<'_> {
    /// Parses a hexadecimal digit.
    async fn hex_digit(&mut self) -> Result<Option<u32>> {
        if let Some(c) = self.peek_char().await? {
            if let Some(digit) = c.to_digit(16) {
                self.consume_char();
                return Ok(Some(digit));
            }
        }
        Ok(None)
    }

    /// Parses a sequence of hexadecimal digits.
    ///
    /// This function consumes up to `count` hexadecimal digits and returns the
    /// value as a single number. If fewer than `count` digits are found, the
    /// function returns the value of the digits found so far. If no digits are
    /// found, the function returns `Ok(None)`.
    async fn hex_digits(&mut self, count: usize) -> Result<Option<u32>> {
        let Some(digit) = self.hex_digit().await? else {
            return Ok(None);
        };
        let mut value = digit;
        for _ in 1..count {
            let Some(digit) = self.hex_digit().await? else {
                break;
            };
            value = value << 4 | digit;
        }
        Ok(Some(value))
    }

    /// Parses an escape unit.
    ///
    /// This function tests if the next character is an escape sequence and
    /// returns it if it is. If the next character is not an escape sequence, it
    /// returns as `EscapeUnit::Literal`. If there is no next character, it
    /// returns `Ok(None)`. It returns an error if an invalid escape sequence is
    /// found.
    ///
    /// This function should be called in a context where [line continuations are
    /// disabled](Self::disable_line_continuation), so that backslash-newline
    /// pairs are not removed before they are parsed as escape sequences.
    pub async fn escape_unit(&mut self) -> Result<Option<EscapeUnit>> {
        let Some(c1) = self.peek_char().await? else {
            return Ok(None);
        };
        let start_index = self.index();
        self.consume_char();
        if c1 != '\\' {
            return Ok(Some(Literal(c1)));
        }

        let Some(c2) = self.peek_char().await? else {
            let cause = SyntaxError::IncompleteEscape.into();
            let location = self.location().await?.clone();
            return Err(Error { cause, location });
        };
        self.consume_char();
        match c2 {
            '"' => Ok(Some(DoubleQuote)),
            '\'' => Ok(Some(SingleQuote)),
            '\\' => Ok(Some(Backslash)),
            '?' => Ok(Some(Question)),
            'a' => Ok(Some(Alert)),
            'b' => Ok(Some(Backspace)),
            'e' | 'E' => Ok(Some(Escape)),
            'f' => Ok(Some(FormFeed)),
            'n' => Ok(Some(Newline)),
            'r' => Ok(Some(CarriageReturn)),
            't' => Ok(Some(Tab)),
            'v' => Ok(Some(VerticalTab)),

            'c' => {
                let start_index = self.index();
                let Some(c3) = self.peek_char().await? else {
                    let cause = SyntaxError::IncompleteControlEscape.into();
                    let location = self.location().await?.clone();
                    return Err(Error { cause, location });
                };
                self.consume_char();
                match c3.to_ascii_uppercase() {
                    '\\' => {
                        let Some('\\') = self.peek_char().await? else {
                            let cause = SyntaxError::IncompleteControlBackslashEscape.into();
                            let location = self.location().await?.clone();
                            return Err(Error { cause, location });
                        };
                        self.consume_char();
                        Ok(Some(Control(0x1C)))
                    }

                    c3 @ ('\u{3F}'..'\u{60}') => Ok(Some(Control(c3 as u8 ^ 0x40))),

                    _ => {
                        let cause = SyntaxError::InvalidControlEscape.into();
                        let location = self.location_range(start_index..self.index());
                        Err(Error { cause, location })
                    }
                }
            }

            'x' => {
                let Some(value) = self.hex_digits(2).await? else {
                    let cause = SyntaxError::IncompleteHexEscape.into();
                    let location = self.location().await?.clone();
                    return Err(Error { cause, location });
                };
                // TODO Reject a third hexadecimal digit in POSIX mode
                Ok(Some(Hex(value as u8)))
            }

            'u' => {
                let Some(value) = self.hex_digits(4).await? else {
                    let cause = SyntaxError::IncompleteShortUnicodeEscape.into();
                    let location = self.location().await?.clone();
                    return Err(Error { cause, location });
                };
                if let Some(c) = char::from_u32(value) {
                    Ok(Some(Unicode(c)))
                } else {
                    let cause = SyntaxError::UnicodeEscapeOutOfRange.into();
                    let location = self.location_range(start_index..self.index());
                    Err(Error { cause, location })
                }
            }

            'U' => {
                let Some(value) = self.hex_digits(8).await? else {
                    let cause = SyntaxError::IncompleteLongUnicodeEscape.into();
                    let location = self.location().await?.clone();
                    return Err(Error { cause, location });
                };
                if let Some(c) = char::from_u32(value) {
                    Ok(Some(Unicode(c)))
                } else {
                    let cause = SyntaxError::UnicodeEscapeOutOfRange.into();
                    let location = self.location_range(start_index..self.index());
                    Err(Error { cause, location })
                }
            }

            _ => {
                // Consume at most 3 octal digits (including c2)
                let Some(mut value) = c2.to_digit(8) else {
                    let cause = SyntaxError::InvalidEscape.into();
                    let location = self.location_range(start_index..self.index());
                    return Err(Error { cause, location });
                };
                for _ in 0..2 {
                    let Some(digit) = self.peek_char().await? else {
                        break;
                    };
                    let Some(digit) = digit.to_digit(8) else {
                        break;
                    };
                    value = value * 8 + digit;
                    self.consume_char();
                }
                if let Ok(value) = value.try_into() {
                    Ok(Some(Octal(value)))
                } else {
                    let cause = SyntaxError::OctalEscapeOutOfRange.into();
                    let location = self.location_range(start_index..self.index());
                    Err(Error { cause, location })
                }
            }
        }
    }

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
        let mut this = self.disable_line_continuation();
        let mut units = Vec::new();

        while let Some(c) = this.peek_char().await? {
            if is_delimiter(c) {
                break;
            }
            let Some(unit) = this.escape_unit().await? else {
                break;
            };
            units.push(unit);
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
        let Some(quote) = self.consume_char_if(is_single_quote).await? else {
            return Ok(None);
        };
        let opening_location = quote.location.clone();

        let content = self.escaped_string(is_single_quote).await?;

        // Consume the closing single quote
        if let Some(quote) = self.peek_char().await? {
            debug_assert_eq!(quote, '\'');
            self.consume_char();
            Ok(Some(content))
        } else {
            let cause = SyntaxError::UnclosedDollarSingleQuote { opening_location }.into();
            let location = self.location().await?.clone();
            Err(Error { cause, location })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ErrorCause;
    use crate::source::Source;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn escape_unit_literal() {
        let mut lexer = Lexer::from_memory("bar", Source::Unknown);
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Literal('b')));
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('a')));
    }

    #[test]
    fn escape_unit_named_escapes() {
        let mut lexer = Lexer::from_memory(r#"\""\'\\\?\a\b\e\E\f\n\r\t\v"#, Source::Unknown);
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(DoubleQuote));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Literal('"')));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(SingleQuote));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Backslash));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Question));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Alert));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Backspace));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Escape));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Escape));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(FormFeed));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Newline));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(CarriageReturn));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Tab));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(VerticalTab));
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn escape_unit_incomplete_escapes() {
        let mut lexer = Lexer::from_memory(r"\", Source::Unknown);
        let error = lexer.escape_unit().now_or_never().unwrap().unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::Syntax(SyntaxError::IncompleteEscape)
        );
        assert_eq!(*error.location.code.value.borrow(), r"\");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 1..1);
    }

    #[test]
    fn escape_unit_control_escapes() {
        let mut lexer = Lexer::from_memory(r"\cA\cz\c^\c?\c\\", Source::Unknown);
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Control(0x01)));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Control(0x1A)));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Control(0x1E)));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Control(0x7F)));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Control(0x1C)));
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn escape_unit_incomplete_control_escape() {
        let mut lexer = Lexer::from_memory(r"\c", Source::Unknown);
        let error = lexer.escape_unit().now_or_never().unwrap().unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::Syntax(SyntaxError::IncompleteControlEscape)
        );
        assert_eq!(*error.location.code.value.borrow(), r"\c");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 2..2);
    }

    #[test]
    fn escape_unit_incomplete_control_backslash_escapes() {
        let mut lexer = Lexer::from_memory(r"\c\", Source::Unknown);
        let error = lexer.escape_unit().now_or_never().unwrap().unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::Syntax(SyntaxError::IncompleteControlBackslashEscape)
        );
        assert_eq!(*error.location.code.value.borrow(), r"\c\");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 3..3);

        let mut lexer = Lexer::from_memory(r"\c\a", Source::Unknown);
        let error = lexer.escape_unit().now_or_never().unwrap().unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::Syntax(SyntaxError::IncompleteControlBackslashEscape)
        );
        assert_eq!(*error.location.code.value.borrow(), r"\c\a");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 3..4);
    }

    #[test]
    fn escape_unit_unknown_control_escape() {
        let mut lexer = Lexer::from_memory(r"\c!`", Source::Unknown);
        let error = lexer.escape_unit().now_or_never().unwrap().unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::Syntax(SyntaxError::InvalidControlEscape)
        );
        assert_eq!(*error.location.code.value.borrow(), r"\c!`");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 2..3);
    }

    #[test]
    fn escape_unit_octal_escapes() {
        let mut lexer = Lexer::from_memory(r"\0\07\234\0123", Source::Unknown);
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Octal(0o0)));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Octal(0o7)));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Octal(0o234)));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Octal(0o12)));
        // At most 3 octal digits are consumed
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('3')));

        let mut lexer = Lexer::from_memory(r"\787", Source::Unknown);
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        // '8' is not an octal digit
        assert_eq!(result, Some(Octal(0o7)));

        let mut lexer = Lexer::from_memory(r"\12", Source::Unknown);
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        // Reaching the end of the input is okay
        assert_eq!(result, Some(Octal(0o12)));
    }

    #[test]
    fn escape_unit_non_byte_octal_escape() {
        let mut lexer = Lexer::from_memory(r"\400", Source::Unknown);
        let error = lexer.escape_unit().now_or_never().unwrap().unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::Syntax(SyntaxError::OctalEscapeOutOfRange)
        );
        assert_eq!(*error.location.code.value.borrow(), r"\400");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 0..4);
    }

    #[test]
    fn escape_unit_hexadecimal_escapes() {
        let mut lexer = Lexer::from_memory(r"\x0\x7F\xd4A", Source::Unknown);
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Hex(0x0)));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Hex(0x7F)));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        // At most 2 hexadecimal digits are consumed
        assert_eq!(result, Some(Hex(0xD4)));
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('A')));

        let mut lexer = Lexer::from_memory(r"\xb", Source::Unknown);
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        // Reaching the end of the input is okay
        assert_eq!(result, Some(Hex(0xB)));
    }

    #[test]
    fn escape_unit_incomplete_hexadecimal_escape() {
        let mut lexer = Lexer::from_memory(r"\x", Source::Unknown);
        let error = lexer.escape_unit().now_or_never().unwrap().unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::Syntax(SyntaxError::IncompleteHexEscape)
        );
        assert_eq!(*error.location.code.value.borrow(), r"\x");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 2..2);
    }

    #[test]
    fn escape_unit_unicode_escapes() {
        let mut lexer = Lexer::from_memory(r"\u20\u4B9d0", Source::Unknown);
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Unicode('\u{20}')));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Unicode('\u{4B9D}')));
        // At most 4 hexadecimal digits are consumed
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('0')));

        let mut lexer = Lexer::from_memory(r"\U42\U0001f4A9b", Source::Unknown);
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Unicode('\u{42}')));
        let result = lexer.escape_unit().now_or_never().unwrap().unwrap();
        assert_eq!(result, Some(Unicode('\u{1F4A9}')));
        // At most 8 hexadecimal digits are consumed
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('b')));
    }

    #[test]
    fn escape_unit_incomplete_unicode_escapes() {
        let mut lexer = Lexer::from_memory(r"\u", Source::Unknown);
        let error = lexer.escape_unit().now_or_never().unwrap().unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::Syntax(SyntaxError::IncompleteShortUnicodeEscape)
        );
        assert_eq!(*error.location.code.value.borrow(), r"\u");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 2..2);

        let mut lexer = Lexer::from_memory(r"\U", Source::Unknown);
        let error = lexer.escape_unit().now_or_never().unwrap().unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::Syntax(SyntaxError::IncompleteLongUnicodeEscape)
        );
        assert_eq!(*error.location.code.value.borrow(), r"\U");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 2..2);
    }

    #[test]
    fn escape_unit_invalid_unicode_escapes() {
        // U+D800 is not a valid Unicode scalar value
        let mut lexer = Lexer::from_memory(r"\uD800", Source::Unknown);
        let error = lexer.escape_unit().now_or_never().unwrap().unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::Syntax(SyntaxError::UnicodeEscapeOutOfRange)
        );
        assert_eq!(*error.location.code.value.borrow(), r"\uD800");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 0..6);
    }

    #[test]
    fn escape_unit_unknown_escape() {
        let mut lexer = Lexer::from_memory(r"\!", Source::Unknown);
        let error = lexer.escape_unit().now_or_never().unwrap().unwrap_err();
        assert_matches!(error.cause, ErrorCause::Syntax(SyntaxError::InvalidEscape));
        assert_eq!(*error.location.code.value.borrow(), r"\!");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 0..2);
    }

    // TODO Reject non-portable escapes in POSIX mode

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
    fn escaped_string_mixed() {
        let mut lexer = Lexer::from_memory(r"foo\bar", Source::Unknown);
        let EscapedString(content) = lexer
            .escaped_string(|_| false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(
            content,
            [
                Literal('f'),
                Literal('o'),
                Literal('o'),
                Backspace,
                Literal('a'),
                Literal('r')
            ]
        );
    }

    #[test]
    fn no_line_continuations_in_escaped_string() {
        let mut lexer = Lexer::from_memory("\\\\\n", Source::Unknown);
        let EscapedString(content) = lexer
            .escaped_string(|_| false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(content, [Backslash, Literal('\n')]);

        let mut lexer = Lexer::from_memory("\\\n", Source::Unknown);
        let error = lexer
            .escaped_string(|_| false)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_matches!(error.cause, ErrorCause::Syntax(SyntaxError::InvalidEscape));
        assert_eq!(*error.location.code.value.borrow(), "\\\n");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 0..2);
    }

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

    #[test]
    fn single_quoted_escaped_string_unclosed() {
        let mut lexer = Lexer::from_memory("'foo", Source::Unknown);
        let error = lexer
            .single_quoted_escaped_string()
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_matches!(
            error.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedDollarSingleQuote { opening_location }) => {
                assert_eq!(*opening_location.code.value.borrow(), "'foo");
                assert_eq!(opening_location.code.start_line_number.get(), 1);
                assert_eq!(*opening_location.code.source, Source::Unknown);
                assert_eq!(opening_location.range, 0..1);
            }
        );
        assert_eq!(*error.location.code.value.borrow(), "'foo");
        assert_eq!(error.location.code.start_line_number.get(), 1);
        assert_eq!(*error.location.code.source, Source::Unknown);
        assert_eq!(error.location.range, 4..4);
    }
}
