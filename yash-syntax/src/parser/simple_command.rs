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

//! Syntax parser for simple command

use super::core::Error;
use super::core::Parser;
use super::core::Result;
use super::core::SyntaxError;
use super::lex::Operator::{CloseParen, Newline, OpenParen};
use super::lex::TokenId::{Operator, Token};
use crate::syntax::Word;

impl Parser<'_> {
    /// Parses the value of an array assignment.
    ///
    /// This function first consumes a `(` token, then any number of words
    /// separated by blanks and/or newlines, and finally a `)`.
    /// If the first token is not `(`, the result is `Ok(None)`.
    /// If the last `)` is missing, the result is
    /// `Err(ErrorCause::Syntax(SyntaxError::UnclosedArrayValue(_)))`.
    pub async fn array_values(&mut self) -> Result<Option<Vec<Word>>> {
        if self.peek_token().await?.id != Operator(OpenParen) {
            return Ok(None);
        }

        let opening_location = self.take_token_raw().await?.word.location;
        let mut words = vec![];

        loop {
            let next = self.take_token_auto(&[]).await?;
            match next.id {
                Operator(Newline) => continue,
                Operator(CloseParen) => break,
                Token(_keyword) => words.push(next.word),
                _ => {
                    return Err(Error {
                        cause: SyntaxError::UnclosedArrayValue { opening_location }.into(),
                        location: next.word.location,
                    })
                }
            }
        }

        Ok(Some(words))
    }
}

#[cfg(test)]
mod tests {
    use super::super::core::ErrorCause;
    use super::super::lex::Lexer;
    use super::super::lex::TokenId::EndOfInput;
    use super::*;
    use crate::source::Source;
    use futures::executor::block_on;

    #[test]
    fn parser_array_values_no_open_parenthesis() {
        let mut lexer = Lexer::with_source(Source::Unknown, ")");
        let mut parser = Parser::new(&mut lexer);
        let result = block_on(parser.array_values()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn parser_array_values_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "()");
        let mut parser = Parser::new(&mut lexer);
        let words = block_on(parser.array_values()).unwrap().unwrap();
        assert_eq!(words, []);

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_array_values_many() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(a b c)");
        let mut parser = Parser::new(&mut lexer);
        let words = block_on(parser.array_values()).unwrap().unwrap();
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].to_string(), "a");
        assert_eq!(words[1].to_string(), "b");
        assert_eq!(words[2].to_string(), "c");
    }

    #[test]
    fn parser_array_values_newlines_and_comments() {
        let mut lexer = Lexer::with_source(
            Source::Unknown,
            "(
            a # b
            c d
        )",
        );
        let mut parser = Parser::new(&mut lexer);
        let words = block_on(parser.array_values()).unwrap().unwrap();
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].to_string(), "a");
        assert_eq!(words[1].to_string(), "c");
        assert_eq!(words[2].to_string(), "d");
    }

    #[test]
    fn parser_array_values_unclosed() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(a b");
        let mut parser = Parser::new(&mut lexer);
        let e = block_on(parser.array_values()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedArrayValue { opening_location }) = e.cause {
            assert_eq!(opening_location.line.value, "(a b");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("Unexpected cause {:?}", e.cause);
        }
        assert_eq!(e.location.line.value, "(a b");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 5);
    }

    #[test]
    fn parser_array_values_invalid_word() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(a;b)");
        let mut parser = Parser::new(&mut lexer);
        let e = block_on(parser.array_values()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedArrayValue { opening_location }) = e.cause {
            assert_eq!(opening_location.line.value, "(a;b)");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("Unexpected cause {:?}", e.cause);
        }
        assert_eq!(e.location.line.value, "(a;b)");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 3);
    }
}
