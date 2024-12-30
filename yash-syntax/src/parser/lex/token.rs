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

//! Part of the lexer that parses backquotes.

use super::core::is_blank;
use super::core::Lexer;
use super::core::Token;
use super::core::TokenId;
use super::core::WordContext;
use super::core::WordLexer;
use super::op::is_operator_char;
use crate::parser::core::Result;
use crate::syntax::MaybeLiteral;
use crate::syntax::Word;

/// Tests whether the given character is a token delimiter.
///
/// A character is a token delimiter if it is either a whitespace or [operator](is_operator_char).
pub fn is_token_delimiter_char(c: char) -> bool {
    is_operator_char(c) || is_blank(c)
}

impl Lexer<'_> {
    /// Determines the token ID for the word.
    ///
    /// This is a helper function used by [`Lexer::token`] and does not support
    /// operators.
    async fn token_id(&mut self, word: &Word) -> Result<TokenId> {
        if word.units.is_empty() {
            return Ok(TokenId::EndOfInput);
        }

        if let Some(literal) = word.to_string_if_literal() {
            if let Ok(keyword) = literal.parse() {
                return Ok(TokenId::Token(Some(keyword)));
            }

            if literal.chars().all(|c| c.is_ascii_digit()) {
                if let Some(next) = self.peek_char().await? {
                    if next == '<' || next == '>' {
                        return Ok(TokenId::IoNumber);
                    }
                }
            }
        }

        Ok(TokenId::Token(None))
    }

    /// Parses a token.
    ///
    /// If there is no more token that can be parsed, the result is a token with an empty word and
    /// [`EndOfInput`](TokenId::EndOfInput) token identifier.
    pub async fn token(&mut self) -> Result<Token> {
        if let Some(op) = self.operator().await? {
            return Ok(op);
        }

        let index = self.index();

        let mut word_lexer = WordLexer {
            lexer: self,
            context: WordContext::Word,
        };
        let mut word = word_lexer.word(is_token_delimiter_char).await?;
        word.parse_tilde_front();

        let id = self.token_id(&word).await?;

        Ok(Token { word, id, index })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use crate::syntax::TextUnit;
    use crate::syntax::WordUnit;
    use futures_util::FutureExt;

    #[test]
    fn lexer_token_empty() {
        // If there's no word unit that can be parsed, it is the end of input.
        let mut lexer = Lexer::with_code("");

        let t = lexer.token().now_or_never().unwrap().unwrap();
        assert_eq!(*t.word.location.code.value.borrow(), "");
        assert_eq!(t.word.location.code.start_line_number.get(), 1);
        assert_eq!(*t.word.location.code.source, Source::Unknown);
        assert_eq!(t.word.location.range, 0..0);
        assert_eq!(t.id, TokenId::EndOfInput);
        assert_eq!(t.index, 0);
    }

    #[test]
    fn lexer_token_non_empty() {
        let mut lexer = Lexer::with_code("abc ");

        let t = lexer.token().now_or_never().unwrap().unwrap();
        assert_eq!(t.word.units.len(), 3);
        assert_eq!(t.word.units[0], WordUnit::Unquoted(TextUnit::Literal('a')));
        assert_eq!(t.word.units[1], WordUnit::Unquoted(TextUnit::Literal('b')));
        assert_eq!(t.word.units[2], WordUnit::Unquoted(TextUnit::Literal('c')));
        assert_eq!(*t.word.location.code.value.borrow(), "abc ");
        assert_eq!(t.word.location.code.start_line_number.get(), 1);
        assert_eq!(*t.word.location.code.source, Source::Unknown);
        assert_eq!(t.word.location.range, 0..3);
        assert_eq!(t.id, TokenId::Token(None));
        assert_eq!(t.index, 0);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(' ')));
    }

    #[test]
    fn lexer_token_tilde() {
        let mut lexer = Lexer::with_code("~a:~");

        let t = lexer.token().now_or_never().unwrap().unwrap();
        assert_eq!(t.word.units, [WordUnit::Tilde("a:~".to_string())]);
    }

    #[test]
    fn lexer_token_io_number_delimited_by_less() {
        let mut lexer = Lexer::with_code("12<");

        let t = lexer.token().now_or_never().unwrap().unwrap();
        assert_eq!(t.word.units.len(), 2);
        assert_eq!(t.word.units[0], WordUnit::Unquoted(TextUnit::Literal('1')));
        assert_eq!(t.word.units[1], WordUnit::Unquoted(TextUnit::Literal('2')));
        assert_eq!(*t.word.location.code.value.borrow(), "12<");
        assert_eq!(t.word.location.code.start_line_number.get(), 1);
        assert_eq!(*t.word.location.code.source, Source::Unknown);
        assert_eq!(t.word.location.range, 0..2);
        assert_eq!(t.id, TokenId::IoNumber);
        assert_eq!(t.index, 0);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_token_io_number_delimited_by_greater() {
        let mut lexer = Lexer::with_code("0>>");

        let t = lexer.token().now_or_never().unwrap().unwrap();
        assert_eq!(t.word.units.len(), 1);
        assert_eq!(t.word.units[0], WordUnit::Unquoted(TextUnit::Literal('0')));
        assert_eq!(*t.word.location.code.value.borrow(), "0>>");
        assert_eq!(t.word.location.code.start_line_number.get(), 1);
        assert_eq!(*t.word.location.code.source, Source::Unknown);
        assert_eq!(t.word.location.range, 0..1);
        assert_eq!(t.id, TokenId::IoNumber);
        assert_eq!(t.index, 0);

        assert_eq!(
            lexer.location().now_or_never().unwrap().unwrap().range,
            1..2
        );
    }

    #[test]
    fn lexer_token_after_blank() {
        let mut lexer = Lexer::with_code(" a  ");

        lexer.skip_blanks().now_or_never().unwrap().unwrap();
        let t = lexer.token().now_or_never().unwrap().unwrap();
        assert_eq!(*t.word.location.code.value.borrow(), " a  ");
        assert_eq!(t.word.location.code.start_line_number.get(), 1);
        assert_eq!(*t.word.location.code.source, Source::Unknown);
        assert_eq!(t.word.location.range, 1..2);
        assert_eq!(t.id, TokenId::Token(None));
        assert_eq!(t.index, 1);

        lexer.skip_blanks().now_or_never().unwrap().unwrap();
        let t = lexer.token().now_or_never().unwrap().unwrap();
        assert_eq!(*t.word.location.code.value.borrow(), " a  ");
        assert_eq!(t.word.location.code.start_line_number.get(), 1);
        assert_eq!(*t.word.location.code.source, Source::Unknown);
        assert_eq!(t.word.location.range, 4..4);
        assert_eq!(t.id, TokenId::EndOfInput);
        assert_eq!(t.index, 4);
    }
}
