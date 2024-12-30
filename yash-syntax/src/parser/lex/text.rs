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

//! Part of the lexer that parses texts

use super::core::Lexer;
use super::core::WordContext;
use super::core::WordLexer;
use crate::parser::core::Result;
use crate::parser::error::Error;
use crate::parser::error::SyntaxError;
use crate::syntax::Backslashed;
use crate::syntax::Literal;
use crate::syntax::Text;
use crate::syntax::TextUnit;

impl WordLexer<'_, '_> {
    /// Parses a [`TextUnit`].
    ///
    /// This function parses a literal character, backslash-escaped character,
    /// [dollar unit](WordLexer::dollar_unit), or
    /// [backquote](WordLexer::backquote).
    ///
    /// `is_delimiter` is a function that decides if a character is a delimiter.
    /// An unquoted character is parsed only if `is_delimiter` returns false for
    /// it.
    ///
    /// `is_escapable` decides if a character can be escaped by a backslash. When
    /// `is_escapable` returns false, the preceding backslash is considered
    /// literal.
    ///
    /// If the text unit is a backquote, treatment of `\"` inside the backquote
    /// depends on `self.context`. If it is `Text`, `\"` is an escaped
    /// double-quote. If `Word`, `\"` is treated literally.
    pub async fn text_unit<F, G>(
        &mut self,
        mut is_delimiter: F,
        mut is_escapable: G,
    ) -> Result<Option<TextUnit>>
    where
        F: FnMut(char) -> bool,
        G: FnMut(char) -> bool,
    {
        self.text_unit_dyn(&mut is_delimiter, &mut is_escapable)
            .await
    }

    /// Dynamic version of [`Self::text_unit`]
    async fn text_unit_dyn(
        &mut self,
        is_delimiter: &mut dyn FnMut(char) -> bool,
        is_escapable: &mut dyn FnMut(char) -> bool,
    ) -> Result<Option<TextUnit>> {
        if self.skip_if(|c| c == '\\').await? {
            if let Some(c) = self.consume_raw_char_if_dyn(is_escapable).await? {
                return Ok(Some(Backslashed(c)));
            } else {
                return Ok(Some(Literal('\\')));
            }
        }

        if let Some(u) = self.dollar_unit().await? {
            return Ok(Some(u));
        }

        if let Some(u) = self.backquote().await? {
            return Ok(Some(u));
        }

        if let Some(sc) = self.consume_char_if(|c| !is_delimiter(c)).await? {
            return Ok(Some(Literal(sc.value)));
        }

        Ok(None)
    }

    /// Like `consume_char_if_dyn`, but ignores line continuation.
    async fn consume_raw_char_if_dyn(
        &mut self,
        is_escapable: &mut dyn FnMut(char) -> bool,
    ) -> Result<Option<char>> {
        Ok(self
            .disable_line_continuation()
            .consume_char_if_dyn(is_escapable)
            .await?
            .map(|c| c.value))
    }
}

impl Lexer<'_> {
    /// Parses a text, i.e., a (possibly empty) sequence of [`TextUnit`]s.
    ///
    /// `is_delimiter` tests if an unquoted character is a delimiter. When
    /// `is_delimiter` returns true, the parser stops parsing and returns the
    /// text up to the delimiter.
    ///
    /// `is_escapable` tests if a backslash can escape a character. When the
    /// parser founds an unquoted backslash, the next character is passed to
    /// `is_escapable`. If `is_escapable` returns true, the backslash is treated
    /// as a valid escape (`TextUnit::Backslashed`). Otherwise, it ia a
    /// literal (`TextUnit::Literal`).
    ///
    /// `is_escapable` also affects escaping of double-quotes inside backquotes.
    /// See [`text_unit`](WordLexer::text_unit) for details. Note that this
    /// function calls `text_unit` with [`WordContext::Text`].
    pub async fn text<F, G>(&mut self, mut is_delimiter: F, mut is_escapable: G) -> Result<Text>
    where
        F: FnMut(char) -> bool,
        G: FnMut(char) -> bool,
    {
        self.text_dyn(&mut is_delimiter, &mut is_escapable).await
    }

    /// Dynamic version of [`Self::text`]
    async fn text_dyn(
        &mut self,
        is_delimiter: &mut dyn FnMut(char) -> bool,
        is_escapable: &mut dyn FnMut(char) -> bool,
    ) -> Result<Text> {
        let mut units = vec![];

        let mut word_lexer = WordLexer {
            lexer: self,
            context: WordContext::Text,
        };
        while let Some(unit) = word_lexer.text_unit_dyn(is_delimiter, is_escapable).await? {
            units.push(unit);
        }

        Ok(Text(units))
    }

    /// Parses a text that may contain nested parentheses.
    ///
    /// This function works similarly to [`text`](Self::text). However, if an
    /// unquoted `(` is found in the text, all text units are parsed up to the
    /// next matching unquoted `)`. Inside the parentheses, the `is_delimiter`
    /// function is ignored and all non-special characters are parsed as literal
    /// word units. After finding the `)`, this function continues parsing to
    /// find a delimiter (as per `is_delimiter`) or another parentheses.
    ///
    /// Nested parentheses are supported: the number of `(`s and `)`s must
    /// match. In other words, the final delimiter is recognized only outside
    /// outermost parentheses.
    pub async fn text_with_parentheses<F, G>(
        &mut self,
        mut is_delimiter: F,
        mut is_escapable: G,
    ) -> Result<Text>
    where
        F: FnMut(char) -> bool,
        G: FnMut(char) -> bool,
    {
        self.text_with_parentheses_dyn(&mut is_delimiter, &mut is_escapable)
            .await
    }

    /// Dynamic version of [`Self::text_with_parentheses`]
    async fn text_with_parentheses_dyn(
        &mut self,
        is_delimiter: &mut dyn FnMut(char) -> bool,
        is_escapable: &mut dyn FnMut(char) -> bool,
    ) -> Result<Text> {
        let mut units = Vec::new();
        let mut open_paren_locations = Vec::new();
        loop {
            let mut is_delimiter_or_paren = |c| {
                if c == '(' {
                    return true;
                }
                if open_paren_locations.is_empty() {
                    is_delimiter(c)
                } else {
                    c == ')'
                }
            };
            let next_units = self
                .text_dyn(&mut is_delimiter_or_paren, is_escapable)
                .await?
                .0;

            units.extend(next_units);

            if let Some(sc) = self.consume_char_if(|c| c == '(').await? {
                units.push(Literal('('));
                open_paren_locations.push(sc.location.clone());
            } else if let Some(opening_location) = open_paren_locations.pop() {
                if self.skip_if(|c| c == ')').await? {
                    units.push(Literal(')'));
                } else {
                    let cause = SyntaxError::UnclosedParen { opening_location }.into();
                    let location = self.location().await?.clone();
                    return Err(Error { cause, location });
                }
            } else {
                break;
            }
        }
        Ok(Text(units))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorCause;
    use crate::source::Source;
    use crate::syntax::Backquote;
    use crate::syntax::BackquoteUnit;
    use crate::syntax::CommandSubst;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn lexer_text_unit_literal_accepted() {
        let mut lexer = Lexer::with_code("X");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let mut called = false;
        let result = lexer
            .text_unit(
                |c| {
                    called = true;
                    assert_eq!(c, 'X');
                    false
                },
                |c| unreachable!("unexpected call to is_escapable({:?})", c),
            )
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(called);
        assert_matches!(result, Literal('X'));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_text_unit_literal_rejected() {
        let mut lexer = Lexer::with_code(";");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let mut called = false;
        let result = lexer
            .text_unit(
                |c| {
                    called = true;
                    assert_eq!(c, ';');
                    true
                },
                |c| unreachable!("unexpected call to is_escapable({:?})", c),
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        assert!(called);
        assert_eq!(result, None);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(';')));
    }

    #[test]
    fn lexer_text_unit_backslash_accepted() {
        let mut lexer = Lexer::with_code(r"\#");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let mut called = false;
        let result = lexer
            .text_unit(
                |c| unreachable!("unexpected call to is_delimiter({:?})", c),
                |c| {
                    called = true;
                    assert_eq!(c, '#');
                    true
                },
            )
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(called);
        assert_eq!(result, Backslashed('#'));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_text_unit_backslash_eof() {
        let mut lexer = Lexer::with_code(r"\");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .text_unit(
                |c| unreachable!("unexpected call to is_delimiter({:?})", c),
                |c| unreachable!("unexpected call to is_escapable({:?})", c),
            )
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(result, Literal('\\'));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_text_unit_backslash_line_continuation_not_recognized() {
        let mut lexer = Lexer::with_code("\\\\\n");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let mut called = false;
        let result = lexer
            .text_unit(
                |c| unreachable!("unexpected call to is_delimiter({:?})", c),
                |c| {
                    called = true;
                    assert_eq!(c, '\\');
                    true
                },
            )
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(called);
        assert_eq!(result, Backslashed('\\'));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('\n')));
    }

    #[test]
    fn lexer_text_unit_dollar() {
        let mut lexer = Lexer::with_code("$()");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .text_unit(
                |c| unreachable!("unexpected call to is_delimiter({:?})", c),
                |c| unreachable!("unexpected call to is_escapable({:?})", c),
            )
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, CommandSubst { content, location } => {
            assert_eq!(&*content, "");
            assert_eq!(location.range, 0..3);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_text_unit_backquote_double_quote_escapable() {
        let mut lexer = Lexer::with_code(r#"`\"`"#);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Text,
        };
        let result = lexer
            .text_unit(
                |c| unreachable!("unexpected call to is_delimiter({:?})", c),
                |c| unreachable!("unexpected call to is_escapable({:?})", c),
            )
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, Backquote { content, location } => {
            assert_eq!(content, [BackquoteUnit::Backslashed('"')]);
            assert_eq!(location.range, 0..4);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_text_unit_backquote_double_quote_not_escapable() {
        let mut lexer = Lexer::with_code(r#"`\"`"#);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .text_unit(
                |c| unreachable!("unexpected call to is_delimiter({:?})", c),
                |c| unreachable!("unexpected call to is_escapable({:?})", c),
            )
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_matches!(result, Backquote { content, location } => {
            assert_eq!(
                content,
                [BackquoteUnit::Literal('\\'), BackquoteUnit::Literal('"')]
            );
            assert_eq!(location.range, 0..4);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_text_unit_line_continuations() {
        let mut lexer = Lexer::with_code("\\\n\\\nX");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = lexer
            .text_unit(
                |_| false,
                |c| unreachable!("unexpected call to is_escapable({:?})", c),
            )
            .now_or_never()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(result, Literal('X'));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_text_empty() {
        let mut lexer = Lexer::with_code("");
        let Text(units) = lexer
            .text(
                |c| unreachable!("unexpected call to is_delimiter({:?})", c),
                |c| unreachable!("unexpected call to is_escapable({:?})", c),
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(units, &[]);
    }

    #[test]
    fn lexer_text_nonempty() {
        let mut lexer = Lexer::with_code("abc");
        let mut called = 0;
        let Text(units) = lexer
            .text(
                |c| {
                    assert!(
                        matches!(c, 'a' | 'b' | 'c'),
                        "unexpected call to is_delimiter({c:?}), called={called}"
                    );
                    called += 1;
                    false
                },
                |c| unreachable!("unexpected call to is_escapable({:?})", c),
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(units, &[Literal('a'), Literal('b'), Literal('c')]);
        assert_eq!(called, 3);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_text_delimiter() {
        let mut lexer = Lexer::with_code("abc");
        let mut called = 0;
        let Text(units) = lexer
            .text(
                |c| {
                    assert!(
                        matches!(c, 'a' | 'b' | 'c'),
                        "unexpected call to is_delimiter({c:?}), called={called}"
                    );
                    called += 1;
                    c == 'c'
                },
                |c| unreachable!("unexpected call to is_escapable({:?})", c),
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(units, &[Literal('a'), Literal('b')]);
        assert_eq!(called, 3);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('c')));
    }

    #[test]
    fn lexer_text_escaping() {
        let mut lexer = Lexer::with_code(r"a\b\c");
        let mut tested_chars = String::new();
        let Text(units) = lexer
            .text(
                |_| false,
                |c| {
                    tested_chars.push(c);
                    c == 'b'
                },
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(
            units,
            &[Literal('a'), Backslashed('b'), Literal('\\'), Literal('c')]
        );
        assert_eq!(tested_chars, "bc");

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_text_with_parentheses_no_parentheses() {
        let mut lexer = Lexer::with_code("abc");
        let Text(units) = lexer
            .text_with_parentheses(|_| false, |_| false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(units, &[Literal('a'), Literal('b'), Literal('c')]);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(None));
    }

    #[test]
    fn lexer_text_with_parentheses_nest_1() {
        let mut lexer = Lexer::with_code("a(b)c)");
        let Text(units) = lexer
            .text_with_parentheses(|c| c == 'b' || c == ')', |_| false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(
            units,
            &[
                Literal('a'),
                Literal('('),
                Literal('b'),
                Literal(')'),
                Literal('c'),
            ]
        );

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(')')));
    }

    #[test]
    fn lexer_text_with_parentheses_nest_1_1() {
        let mut lexer = Lexer::with_code("ab(CD)ef(GH)ij;");
        let Text(units) = lexer
            .text_with_parentheses(|c| c.is_ascii_uppercase() || c == ';', |_| false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(
            units,
            &[
                Literal('a'),
                Literal('b'),
                Literal('('),
                Literal('C'),
                Literal('D'),
                Literal(')'),
                Literal('e'),
                Literal('f'),
                Literal('('),
                Literal('G'),
                Literal('H'),
                Literal(')'),
                Literal('i'),
                Literal('j'),
            ]
        );

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(';')));
    }

    #[test]
    fn lexer_text_with_parentheses_nest_3() {
        let mut lexer = Lexer::with_code("a(B((C)D))e;");
        let Text(units) = lexer
            .text_with_parentheses(|c| c.is_ascii_uppercase() || c == ';', |_| false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(
            units,
            &[
                Literal('a'),
                Literal('('),
                Literal('B'),
                Literal('('),
                Literal('('),
                Literal('C'),
                Literal(')'),
                Literal('D'),
                Literal(')'),
                Literal(')'),
                Literal('e'),
            ]
        );

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(';')));
    }

    #[test]
    fn lexer_text_with_parentheses_unclosed() {
        let mut lexer = Lexer::with_code("x(()");
        let e = lexer
            .text_with_parentheses(|_| false, |_| false)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedParen { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "x(()");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(*opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 1..2);
        });
        assert_eq!(*e.location.code.value.borrow(), "x(()");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 4..4);
    }
}
