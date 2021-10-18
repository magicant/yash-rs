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

//! Syntax parser for redirection

use super::core::Parser;
use super::core::Result;
use super::error::Error;
use super::error::SyntaxError;
use super::fill::MissingHereDoc;
use super::lex::Operator::{LessLess, LessLessDash};
use super::lex::PartialHereDoc;
use super::lex::TokenId::{EndOfInput, IoNumber, Operator, Token};
use crate::source::Location;
use crate::syntax::Fd;
use crate::syntax::Redir;
use crate::syntax::RedirBody;
use crate::syntax::RedirOp;
use crate::syntax::Word;
use std::convert::TryFrom;

impl Parser<'_, '_> {
    /// Parses the operand of a redirection operator.
    async fn redirection_operand(&mut self) -> Result<std::result::Result<Word, Location>> {
        let operand = self.take_token_auto(&[]).await?;
        match operand.id {
            Token(_) => (),
            Operator(_) | EndOfInput => return Ok(Err(operand.word.location)),
            IoNumber => (), // TODO reject if POSIXly-correct
        }
        Ok(Ok(operand.word))
    }

    /// Parses a normal redirection body.
    async fn normal_redirection_body(
        &mut self,
        operator: RedirOp,
    ) -> Result<RedirBody<MissingHereDoc>> {
        // TODO reject >>| and <<< if POSIXly-correct
        self.take_token_raw().await?;
        let operand = self
            .redirection_operand()
            .await?
            .map_err(|location| Error {
                cause: SyntaxError::MissingRedirOperand.into(),
                location,
            })?;
        Ok(RedirBody::Normal { operator, operand })
    }

    /// Parses the redirection body for a here-document.
    async fn here_doc_redirection_body(
        &mut self,
        remove_tabs: bool,
    ) -> Result<RedirBody<MissingHereDoc>> {
        self.take_token_raw().await?;
        let delimiter = self
            .redirection_operand()
            .await?
            .map_err(|location| Error {
                cause: SyntaxError::MissingHereDocDelimiter.into(),
                location,
            })?;

        self.memorize_unread_here_doc(PartialHereDoc {
            delimiter,
            remove_tabs,
        });

        Ok(RedirBody::HereDoc(MissingHereDoc))
    }

    /// Parses the redirection body.
    async fn redirection_body(&mut self) -> Result<Option<RedirBody<MissingHereDoc>>> {
        let operator = match self.peek_token().await?.id {
            Operator(operator) => operator,
            _ => return Ok(None),
        };

        if let Ok(operator) = RedirOp::try_from(operator) {
            return Ok(Some(self.normal_redirection_body(operator).await?));
        }
        match operator {
            LessLess => Ok(Some(self.here_doc_redirection_body(false).await?)),
            LessLessDash => Ok(Some(self.here_doc_redirection_body(true).await?)),
            // TODO <() >()
            _ => Ok(None),
        }
    }

    /// Parses a redirection.
    ///
    /// If the current token is not a redirection operator, `Ok(None)` is returned. If a word token
    /// is missing after the operator, `Err(Error{...})` is returned with a cause of
    /// [`MissingRedirOperand`](SyntaxError::MissingRedirOperand) or
    /// [`MissingHereDocDelimiter`](SyntaxError::MissingHereDocDelimiter).
    pub async fn redirection(&mut self) -> Result<Option<Redir<MissingHereDoc>>> {
        let fd = if self.peek_token().await?.id == IoNumber {
            let token = self.take_token_raw().await?;
            if let Ok(fd) = token.word.to_string().parse() {
                Some(Fd(fd))
            } else {
                return Err(Error {
                    cause: SyntaxError::FdOutOfRange.into(),
                    location: token.word.location,
                });
            }
        } else {
            None
        };

        Ok(self
            .redirection_body()
            .await?
            .map(|body| Redir { fd, body }))
    }

    /// Parses a (possibly empty) sequence of redirections.
    pub async fn redirections(&mut self) -> Result<Vec<Redir<MissingHereDoc>>> {
        // TODO substitute global aliases
        let mut redirs = vec![];
        while let Some(redir) = self.redirection().await? {
            redirs.push(redir);
        }
        Ok(redirs)
    }
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::lex::Lexer;
    use super::super::lex::Operator::Newline;
    use super::*;
    use crate::source::Source;
    use futures_executor::block_on;

    #[test]
    fn parser_redirection_less() {
        let mut lexer = Lexer::from_memory("</dev/null\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        if let RedirBody::Normal { operator, operand } = redir.body {
            assert_eq!(operator, RedirOp::FileIn);
            assert_eq!(operand.to_string(), "/dev/null")
        } else {
            panic!("Unexpected redirection body {:?}", redir.body);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, Operator(Newline));
    }

    #[test]
    fn parser_redirection_less_greater() {
        let mut lexer = Lexer::from_memory("<> /dev/null\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        if let RedirBody::Normal { operator, operand } = redir.body {
            assert_eq!(operator, RedirOp::FileInOut);
            assert_eq!(operand.to_string(), "/dev/null")
        } else {
            panic!("Unexpected redirection body {:?}", redir.body);
        }
    }

    #[test]
    fn parser_redirection_greater() {
        let mut lexer = Lexer::from_memory(">/dev/null\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        if let RedirBody::Normal { operator, operand } = redir.body {
            assert_eq!(operator, RedirOp::FileOut);
            assert_eq!(operand.to_string(), "/dev/null")
        } else {
            panic!("Unexpected redirection body {:?}", redir.body);
        }
    }

    #[test]
    fn parser_redirection_greater_greater() {
        let mut lexer = Lexer::from_memory(" >> /dev/null\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        if let RedirBody::Normal { operator, operand } = redir.body {
            assert_eq!(operator, RedirOp::FileAppend);
            assert_eq!(operand.to_string(), "/dev/null")
        } else {
            panic!("Unexpected redirection body {:?}", redir.body);
        }
    }

    #[test]
    fn parser_redirection_greater_bar() {
        let mut lexer = Lexer::from_memory(">| /dev/null\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        if let RedirBody::Normal { operator, operand } = redir.body {
            assert_eq!(operator, RedirOp::FileClobber);
            assert_eq!(operand.to_string(), "/dev/null")
        } else {
            panic!("Unexpected redirection body {:?}", redir.body);
        }
    }

    #[test]
    fn parser_redirection_less_and() {
        let mut lexer = Lexer::from_memory("<& -\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        if let RedirBody::Normal { operator, operand } = redir.body {
            assert_eq!(operator, RedirOp::FdIn);
            assert_eq!(operand.to_string(), "-")
        } else {
            panic!("Unexpected redirection body {:?}", redir.body);
        }
    }

    #[test]
    fn parser_redirection_greater_and() {
        let mut lexer = Lexer::from_memory(">& 3\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        if let RedirBody::Normal { operator, operand } = redir.body {
            assert_eq!(operator, RedirOp::FdOut);
            assert_eq!(operand.to_string(), "3")
        } else {
            panic!("Unexpected redirection body {:?}", redir.body);
        }
    }

    #[test]
    fn parser_redirection_greater_greater_bar() {
        let mut lexer = Lexer::from_memory(">>| 3\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        if let RedirBody::Normal { operator, operand } = redir.body {
            assert_eq!(operator, RedirOp::Pipe);
            assert_eq!(operand.to_string(), "3")
        } else {
            panic!("Unexpected redirection body {:?}", redir.body);
        }
    }

    #[test]
    fn parser_redirection_less_less_less() {
        let mut lexer = Lexer::from_memory("<<< foo\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        if let RedirBody::Normal { operator, operand } = redir.body {
            assert_eq!(operator, RedirOp::String);
            assert_eq!(operand.to_string(), "foo")
        } else {
            panic!("Unexpected redirection body {:?}", redir.body);
        }
    }

    #[test]
    fn parser_redirection_less_less() {
        let mut lexer = Lexer::from_memory("<<end \nend\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        assert_eq!(redir.body, RedirBody::HereDoc(MissingHereDoc));

        block_on(parser.newline_and_here_doc_contents()).unwrap();
        let here_docs = parser.take_read_here_docs();
        assert_eq!(here_docs.len(), 1);
        assert_eq!(here_docs[0].delimiter.to_string(), "end");
        assert_eq!(here_docs[0].remove_tabs, false);
        assert_eq!(here_docs[0].content.to_string(), "");
    }

    #[test]
    fn parser_redirection_less_less_dash() {
        let mut lexer = Lexer::from_memory("<<-end \nend\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, None);
        assert_eq!(redir.body, RedirBody::HereDoc(MissingHereDoc));

        block_on(parser.newline_and_here_doc_contents()).unwrap();
        let here_docs = parser.take_read_here_docs();
        assert_eq!(here_docs.len(), 1);
        assert_eq!(here_docs[0].delimiter.to_string(), "end");
        assert_eq!(here_docs[0].remove_tabs, true);
        assert_eq!(here_docs[0].content.to_string(), "");
    }

    #[test]
    fn parser_redirection_with_io_number() {
        let mut lexer = Lexer::from_memory("12< /dev/null\n", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let redir = block_on(parser.redirection()).unwrap().unwrap();
        assert_eq!(redir.fd, Some(Fd(12)));
        if let RedirBody::Normal { operator, operand } = redir.body {
            assert_eq!(operator, RedirOp::FileIn);
            assert_eq!(operand.to_string(), "/dev/null")
        } else {
            panic!("Unexpected redirection body {:?}", redir.body);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, Operator(Newline));
    }

    #[test]
    fn parser_redirection_fd_out_of_range() {
        let mut lexer = Lexer::from_memory(
            "9999999999999999999999999999999999999999< x",
            Source::Unknown,
        );
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.redirection()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::FdOutOfRange));
        assert_eq!(
            e.location.line.value,
            "9999999999999999999999999999999999999999< x"
        );
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }

    #[test]
    fn parser_redirection_not_operator() {
        let mut lexer = Lexer::from_memory("x", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        assert!(block_on(parser.redirection()).unwrap().is_none());
    }

    #[test]
    fn parser_redirection_non_word_operand() {
        let mut lexer = Lexer::from_memory(" < >", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.redirection()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingRedirOperand)
        );
        assert_eq!(e.location.line.value, " < >");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 4);
    }

    #[test]
    fn parser_redirection_eof_operand() {
        let mut lexer = Lexer::from_memory("  < ", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.redirection()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingRedirOperand)
        );
        assert_eq!(e.location.line.value, "  < ");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 5);
    }

    #[test]
    fn parser_redirection_not_heredoc_delimiter() {
        let mut lexer = Lexer::from_memory("<< <<", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.redirection()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingHereDocDelimiter)
        );
        assert_eq!(e.location.line.value, "<< <<");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 4);
    }

    #[test]
    fn parser_redirection_eof_heredoc_delimiter() {
        let mut lexer = Lexer::from_memory("<<", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.redirection()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingHereDocDelimiter)
        );
        assert_eq!(e.location.line.value, "<<");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 3);
    }
}
