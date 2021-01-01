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

//! Syntax parser for the shell language.
//!
//! TODO Elaborate

mod core;
mod fill;
mod lex;

use self::lex::Operator::*;
use self::lex::TokenId::*;
use super::syntax::*;

pub use self::core::AsyncFnMut;
pub use self::core::Error;
pub use self::core::ErrorCause;
pub use self::core::Parser;
pub use self::core::Result;
pub use self::fill::Fill;
pub use self::fill::MissingHereDoc;
pub use self::lex::Lexer;
pub use self::lex::Operator;
pub use self::lex::Token;
pub use self::lex::TokenId;

impl Parser<'_> {
    /// Parses a redirection.
    ///
    /// If the current token is not a redirection operator, `Ok(None)` is returned. If a word token
    /// is missing after the operator, `Err(Error{...})` is returned with a cause of
    /// [`MissingHereDocDelimiter`](ErrorCause::MissingHereDocDelimiter).
    pub async fn redirection(&mut self) -> Result<Option<Redir<MissingHereDoc>>> {
        // TODO IO_NUMBER
        let operator = match self.peek_token().await?.id {
            // TODO <, <>, >, >>, >|, <&, >&, >>|, <<<
            Operator(op) if op == LessLess || op == LessLessDash => {
                self.take_token().await.unwrap()
            }
            _ => return Ok(None),
        };

        let operand = self.take_token().await?;
        match operand.id {
            Token => (),
            Operator(_) | EndOfInput => {
                return Err(Error {
                    cause: ErrorCause::MissingHereDocDelimiter,
                    location: operator.word.location,
                })
            } // TODO IoNumber => reject if posixly-correct,
        }

        Ok(Some(Redir {
            fd: None,
            body: RedirBody::HereDoc(MissingHereDoc),
        }))
    }

    /// Parses a simple command.
    pub async fn simple_command(&mut self) -> Result<SimpleCommand<MissingHereDoc>> {
        // TODO Return Option::None if the first token is not a normal word token.
        // TODO Support assignments.
        let mut words = vec![];
        let mut redirs = vec![];
        loop {
            if let Some(redir) = self.redirection().await? {
                redirs.push(redir);
                continue;
            }
            let token = self.peek_token().await?;
            if token.id != Token {
                break;
            }
            words.push(self.take_token().await?.word);
        }
        Ok(SimpleCommand { words, redirs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use futures::executor::block_on;

    #[test]
    fn parser_redirection_lessless() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<end ");
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        assert_eq!(redir.body, RedirBody::HereDoc(MissingHereDoc));
        // TODO pending here-doc content
    }

    #[test]
    fn parser_redirection_lesslessdash() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<-end ");
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        assert_eq!(redir.body, RedirBody::HereDoc(MissingHereDoc));
        // TODO pending here-doc content
    }

    #[test]
    fn parser_redirection_not_operator() {
        let mut lexer = Lexer::with_source(Source::Unknown, "x");
        let mut parser = Parser::new(&mut lexer);

        assert!(block_on(parser.redirection()).unwrap().is_none());
    }

    #[test]
    fn parser_redirection_not_heredoc_delimiter() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<< <<");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.redirection()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::MissingHereDocDelimiter);
        assert_eq!(e.location.line.value, "<< <<");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }

    #[test]
    fn parser_redirection_eof_heredoc_delimiter() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.redirection()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::MissingHereDocDelimiter);
        assert_eq!(e.location.line.value, "<<");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }

    // TODO test simple_command
}
