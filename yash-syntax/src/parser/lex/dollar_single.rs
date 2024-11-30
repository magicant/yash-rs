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

//! Parsing dollar-single-quoted strings

use super::core::Lexer;
use crate::parser::core::Result;
use crate::syntax::EscapeUnit;
use crate::syntax::EscapedString;
use crate::syntax::WordUnit::{self, DollarSingleQuote};

impl Lexer<'_> {
    /// Parses a dollar-single-quoted string.
    ///
    /// The initial `$` must have been consumed before calling this function.
    /// The enclosing `'`s are consumed in this function.
    ///
    /// If the next character is not `'`, this function returns `None`.
    pub(super) async fn dollar_single_quote(&mut self) -> Result<Option<WordUnit>> {
        if self.consume_char_if(|c| c == '\'').await?.is_none() {
            return Ok(None);
        }

        let mut units = Vec::new();
        // Loop until the closing `'` is found
        loop {
            let Some(c) = self.peek_char().await? else {
                todo!("return error");
            };
            self.consume_char();

            use EscapeUnit::*;
            match c {
                '\'' => break,

                // TODO Consider extracting this to a separate function
                '\\' => {
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
                }

                c => units.push(Literal(c)),
            }
        }
        Ok(Some(DollarSingleQuote(EscapedString(units))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn lexer_word_unit_dollar_single_quote_empty() {
        let mut lexer = Lexer::from_memory("''", Source::Unknown);
        let result = lexer.dollar_single_quote().now_or_never().unwrap().unwrap();
        assert_matches!(result, Some(DollarSingleQuote(EscapedString(content))) => {
            assert_eq!(content, []);
        });
    }

    #[test]
    fn lexer_word_unit_dollar_single_quote_literals() {
        let mut lexer = Lexer::from_memory("'foo'", Source::Unknown);
        let result = lexer.dollar_single_quote().now_or_never().unwrap().unwrap();
        assert_matches!(result, Some(DollarSingleQuote(EscapedString(content))) => {
            assert_eq!(
                content,
                [
                    EscapeUnit::Literal('f'),
                    EscapeUnit::Literal('o'),
                    EscapeUnit::Literal('o'),
                ]
            );
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_word_unit_dollar_single_quote_named_escapes() {
        let mut lexer = Lexer::from_memory(r#"'\""\'\\\?\a\b\e\E\f\n\r\t\v'x"#, Source::Unknown);
        let result = lexer.dollar_single_quote().now_or_never().unwrap().unwrap();
        assert_matches!(result, Some(DollarSingleQuote(EscapedString(content))) => {
            assert_eq!(
                content,
                [
                    EscapeUnit::DoubleQuote,
                    EscapeUnit::Literal('"'),
                    EscapeUnit::SingleQuote,
                    EscapeUnit::Backslash,
                    EscapeUnit::Question,
                    EscapeUnit::Alert,
                    EscapeUnit::Backspace,
                    EscapeUnit::Escape,
                    EscapeUnit::Escape,
                    EscapeUnit::FormFeed,
                    EscapeUnit::Newline,
                    EscapeUnit::CarriageReturn,
                    EscapeUnit::Tab,
                    EscapeUnit::VerticalTab,
                ]
            );
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('x')));
    }

    #[test]
    #[ignore = "not implemented"]
    fn lexer_word_unit_dollar_single_quote_incomplete_escapes() {
        todo!()
    }

    #[test]
    fn lexer_word_unit_dollar_single_quote_control_escapes() {
        let mut lexer = Lexer::from_memory(r"'\cA\cz\c^\c?\c\\'", Source::Unknown);
        let result = lexer.dollar_single_quote().now_or_never().unwrap().unwrap();
        assert_matches!(result, Some(DollarSingleQuote(EscapedString(content))) => {
            assert_eq!(
                content,
                [
                    EscapeUnit::Control(0x01),
                    EscapeUnit::Control(0x1A),
                    EscapeUnit::Control(0x1E),
                    EscapeUnit::Control(0x7F),
                    EscapeUnit::Control(0x1C),
                ]
            );
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    #[ignore = "not implemented"]
    fn lexer_word_unit_dollar_single_quote_incomplete_control_escapes() {
        todo!()
    }

    #[test]
    fn lexer_word_unit_dollar_single_quote_octal_escapes() {
        let mut lexer = Lexer::from_memory(r"'\0\07\177\0123'", Source::Unknown);
        let result = lexer.dollar_single_quote().now_or_never().unwrap().unwrap();
        assert_matches!(result, Some(DollarSingleQuote(EscapedString(content))) => {
            assert_eq!(
                content,
                [
                    EscapeUnit::Octal(0o0),
                    EscapeUnit::Octal(0o7),
                    EscapeUnit::Octal(0o177),
                    EscapeUnit::Octal(0o12),
                    EscapeUnit::Literal('3'),
                ]
            );
        });
    }

    #[test]
    #[ignore = "not implemented"]
    fn lexer_word_unit_dollar_single_quote_non_byte_octal_escape() {
        let mut lexer = Lexer::from_memory(r"'\700'", Source::Unknown);
        let result = lexer.dollar_single_quote().now_or_never().unwrap().unwrap();
        todo!("should be an error: {result:?}");
    }

    #[test]
    #[ignore = "not implemented"]
    fn lexer_word_unit_dollar_single_quote_hex_escapes() {
        todo!()
    }

    #[test]
    #[ignore = "not implemented"]
    fn lexer_word_unit_dollar_single_quote_incomplete_hex_escape() {
        todo!()
    }

    #[test]
    #[ignore = "not implemented"]
    fn lexer_word_unit_dollar_single_quote_unicode_escapes() {
        todo!()
    }

    #[test]
    #[ignore = "not implemented"]
    fn lexer_word_unit_dollar_single_quote_incomplete_unicode_escapes() {
        todo!()
    }

    // TODO Reject non-portable escapes in POSIX mode

    // TODO lexer_word_unit_dollar_single_quote_unclosed
    // TODO lexer_word_unit_dollar_single_quote_not_quote_in_text_context
}
