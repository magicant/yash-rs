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

pub mod lex;

use self::lex::keyword::Keyword;
use self::lex::Operator::*;
use self::lex::PartialHereDoc;
use self::lex::Token;
use self::lex::TokenId::*;
use super::syntax::*;
use std::rc::Rc;

pub use self::core::AsyncFnMut;
pub use self::core::Error;
pub use self::core::ErrorCause;
pub use self::core::Parser;
pub use self::core::Rec;
pub use self::core::Result;
pub use self::fill::Fill;
pub use self::fill::MissingHereDoc;

impl Parser<'_> {
    /// Consumes the current token with possible alias substitution fully applied.
    ///
    /// This function calls
    /// [`self.take_token_aliased(false)`](Parser::take_token_aliased) repeatedly
    /// until it returns `Ok(Rec::Parsed(_))` or `Err(_)` and then returns it.
    ///
    /// This function should be used only in contexts where no backtrack is
    /// needed after alias substitution.
    pub async fn take_token_aliased_fully(&mut self) -> Result<Token> {
        loop {
            if let Rec::Parsed(t) = self.take_token_aliased(false).await? {
                return Ok(t);
            }
        }
    }

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

        let operand = self.take_token_aliased_fully().await?;
        match operand.id {
            Token(_) => (),
            Operator(_) | EndOfInput => {
                return Err(Error {
                    cause: ErrorCause::MissingHereDocDelimiter,
                    location: operator.word.location,
                })
            } // TODO IoNumber => reject if posixly-correct,
        }

        let remove_tabs = match operator.id {
            Operator(LessLess) => false,
            Operator(LessLessDash) => true,
            _ => unreachable!("unhandled redirection operator type"),
        };
        self.memorize_unread_here_doc(PartialHereDoc {
            delimiter: operand.word,
            remove_tabs,
        });

        Ok(Some(Redir {
            fd: None,
            body: RedirBody::HereDoc(MissingHereDoc),
        }))
    }

    /// Parses a simple command.
    ///
    /// If there is no valid command at the current position, this function
    /// returns `Ok(Rec::Parsed(None))`.
    pub async fn simple_command(&mut self) -> Result<Rec<Option<SimpleCommand<MissingHereDoc>>>> {
        // TODO Return Option::None if the first token is a keyword.
        // TODO Support assignments.
        let mut words = vec![];
        let mut redirs = vec![];
        loop {
            if let Some(redir) = self.redirection().await? {
                redirs.push(redir);
                continue;
            }

            match self.peek_token().await?.id {
                // TODO Also consider assignments.is_empty
                Token(Some(_)) if words.is_empty() && redirs.is_empty() => break,
                Token(_) => (),
                _ => break,
            }

            match self.take_token_aliased(words.is_empty()).await? {
                // TODO Also consider assignments.is_empty
                Rec::AliasSubstituted => {
                    if words.is_empty() && redirs.is_empty() {
                        return Ok(Rec::AliasSubstituted);
                    }
                }
                Rec::Parsed(token) => words.push(token.word),
            }
        }

        // TODO Also consider assignments.is_empty
        if words.is_empty() && redirs.is_empty() {
            Ok(Rec::Parsed(None))
        } else {
            Ok(Rec::Parsed(Some(SimpleCommand { words, redirs })))
        }
    }

    /// Parses a command.
    ///
    /// If there is no valid command at the current position, this function
    /// returns `Ok(Rec::Parsed(None))`.
    pub async fn command(&mut self) -> Result<Rec<Option<Command<MissingHereDoc>>>> {
        // TODO compound command
        // TODO Function definition
        match self.simple_command().await? {
            Rec::AliasSubstituted => Ok(Rec::AliasSubstituted),
            Rec::Parsed(None) => Ok(Rec::Parsed(None)),
            Rec::Parsed(Some(c)) => Ok(Rec::Parsed(Some(Command::SimpleCommand(c)))),
        }
    }

    /// Parses a pipeline.
    ///
    /// If there is no valid pipeline at the current position, this function
    /// returns `Ok(Rec::Parsed(None))`.
    pub async fn pipeline(&mut self) -> Result<Rec<Option<Pipeline<MissingHereDoc>>>> {
        // Parse the first command
        let (first, negation) = match self.command().await? {
            Rec::AliasSubstituted => return Ok(Rec::AliasSubstituted),
            Rec::Parsed(Some(first)) => (first, false),
            Rec::Parsed(None) => {
                // Parse the `!` reserved word
                if let Token(Some(Keyword::Bang)) = self.peek_token().await?.id {
                    let location = self.take_token().await?.word.location;
                    loop {
                        // Parse the command after the `!`
                        if let Rec::Parsed(option) = self.command().await? {
                            if let Some(first) = option {
                                break (first, true);
                            } else {
                                let cause =
                                    if self.peek_token().await?.id == Token(Some(Keyword::Bang)) {
                                        ErrorCause::DoubleNegation
                                    } else {
                                        ErrorCause::MissingCommandAfterBang
                                    };
                                return Err(Error { cause, location });
                            }
                        }
                    }
                } else {
                    return Ok(Rec::Parsed(None));
                }
            }
        };

        // Parse `|`
        let mut commands = vec![Rc::new(first)];
        while self.peek_token().await?.id == Operator(Bar) {
            let location = self.take_token().await?.word.location;

            while self.newline_and_here_doc_contents().await? {}

            // Parse the next command
            let next = loop {
                if let Rec::Parsed(option) = self.command().await? {
                    if let Some(next) = option {
                        break next;
                    }

                    // Error: the command is missing
                    let next = self.peek_token().await?;
                    return if next.id == Token(Some(Keyword::Bang)) {
                        Err(Error {
                            cause: ErrorCause::BangAfterBar,
                            location: next.word.location.clone(),
                        })
                    } else {
                        Err(Error {
                            cause: ErrorCause::MissingCommandAfterBar,
                            location,
                        })
                    };
                }
            };
            commands.push(Rc::new(next));
        }

        Ok(Rec::Parsed(Some(Pipeline { commands, negation })))
    }

    /// Parses an and-or list.
    ///
    /// If there is no valid and-or list at the current position, this function
    /// returns `Ok(Rec::Parsed(None))`.
    pub async fn and_or_list(&mut self) -> Result<Rec<Option<AndOrList<MissingHereDoc>>>> {
        let first = match self.pipeline().await? {
            Rec::AliasSubstituted => return Ok(Rec::AliasSubstituted),
            Rec::Parsed(None) => return Ok(Rec::Parsed(None)),
            Rec::Parsed(Some(p)) => p,
        };

        let mut rest = vec![];
        loop {
            let condition = match self.peek_token().await?.id {
                Operator(AndAnd) => AndOr::AndThen,
                Operator(BarBar) => AndOr::OrElse,
                _ => break,
            };
            self.take_token().await?;

            while self.newline_and_here_doc_contents().await? {}

            let maybe_pipeline = loop {
                if let Rec::Parsed(maybe_pipeline) = self.pipeline().await? {
                    break maybe_pipeline;
                }
            };
            let pipeline = match maybe_pipeline {
                None => {
                    let cause = ErrorCause::MissingPipeline(condition);
                    let location = self.peek_token().await?.word.location.clone();
                    return Err(Error { cause, location });
                }
                Some(pipeline) => pipeline,
            };

            rest.push((condition, pipeline));
        }

        Ok(Rec::Parsed(Some(AndOrList { first, rest })))
    }

    // There is no function that parses a single item because it would not be
    // very useful for parsing a list. An item requires a separator operator
    // ('&' or ';') for it to be followed by another item. You cannot tell from
    // the resultant item whether there was a separator operator.
    // pub async fn item(&mut self) -> Result<Rec<Item<MissingHereDoc>>> { }

    /// Parses a list.
    ///
    /// This function parses a sequence of and-or lists that are separated by `;`
    /// or `&`. A newline token that delimits the list is not parsed.
    ///
    /// If there is no valid command at the current position, this function
    /// returns a list with no items.
    pub async fn list(&mut self) -> Result<Rec<List<MissingHereDoc>>> {
        let mut items = vec![];

        let mut and_or = match self.and_or_list().await? {
            Rec::AliasSubstituted => return Ok(Rec::AliasSubstituted),
            Rec::Parsed(None) => return Ok(Rec::Parsed(List { items: vec![] })),
            Rec::Parsed(Some(and_or)) => and_or,
        };

        loop {
            let (is_async, next) = match self.peek_token().await?.id {
                Operator(Semicolon) => (false, true),
                Operator(And) => (true, true),
                _ => (false, false),
            };

            items.push(Item { and_or, is_async });

            if !next {
                break;
            }
            self.take_token().await?;

            let result = loop {
                if let Rec::Parsed(result) = self.and_or_list().await? {
                    break result;
                }
            };
            and_or = match result {
                None => break,
                Some(and_or) => and_or,
            }
        }

        Ok(Rec::Parsed(List { items }))
    }

    /// Parses an optional newline token and here-document contents.
    ///
    /// If the current token is a newline, it is consumed and any pending here-document contents
    /// are read starting from the next line. Otherwise, this function returns `Ok(false)` without
    /// any side effect.
    pub async fn newline_and_here_doc_contents(&mut self) -> Result<bool> {
        if self.peek_token().await?.id != Operator(Newline) {
            return Ok(false);
        }

        self.take_token().await?;
        self.here_doc_contents().await?;
        Ok(true)
    }

    /// Parses a complete command optionally delimited by a newline.
    ///
    /// A complete command is a minimal sequence of and-or lists that can be executed in the shell
    /// environment. This function reads as many lines as needed to compose the complete command.
    ///
    /// If the current line is empty (or containing only whitespaces and comments), the result is
    /// an empty list. If the first token of the current line is the end of input, the result is
    /// `Ok(None)`.
    pub async fn command_line(&mut self) -> Result<Option<List>> {
        let list = loop {
            if let Rec::Parsed(list) = self.list().await? {
                break list;
            }
        };

        if !self.newline_and_here_doc_contents().await? {
            let next = self.peek_token().await?;
            if next.id != EndOfInput {
                // TODO Return a better error depending on the token id of the peeked token
                return Err(Error {
                    cause: ErrorCause::UnexpectedToken,
                    location: next.word.location.clone(),
                });
            }
            if list.items.is_empty() {
                return Ok(None);
            }
        }

        self.ensure_no_unread_here_doc()?;
        let mut here_docs = self.take_read_here_docs().into_iter();
        let list = list.fill(&mut here_docs)?;
        Ok(Some(list))
    }

    /// Parses an optional compound list.
    ///
    /// A compound list is a sequence of one or more and-or lists that are
    /// separated by newlines and optionally preceded and/or followed by
    /// newlines.
    ///
    /// This function stops parsing on encountering an unexpected token that
    /// cannot be parsed as the beginning of an and-or list. The caller should
    /// check that the next token is an expected one.
    pub async fn maybe_compound_list(&mut self) -> Result<List<MissingHereDoc>> {
        let mut items = vec![];

        loop {
            let list = loop {
                if let Rec::Parsed(list) = self.list().await? {
                    break list;
                }
            };
            items.extend(list.items);

            if !self.newline_and_here_doc_contents().await? {
                break;
            }
        }

        Ok(List { items })
    }
}

#[cfg(test)]
mod tests {
    use super::lex::Lexer;
    use super::*;
    use crate::source::Source;
    use futures::executor::block_on;

    #[test]
    fn parser_redirection_lessless() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<end \nend\n");
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
    fn parser_redirection_lesslessdash() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<-end \nend\n");
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

    #[test]
    fn parser_simple_command_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut parser = Parser::new(&mut lexer);

        let option = block_on(parser.simple_command()).unwrap().unwrap();
        assert_eq!(option, None);
    }

    #[test]
    fn parser_simple_command_keyword() {
        let mut lexer = Lexer::with_source(Source::Unknown, "then");
        let mut parser = Parser::new(&mut lexer);

        let option = block_on(parser.simple_command()).unwrap().unwrap();
        assert_eq!(option, None);
    }

    // TODO test simple_command for other cases

    #[test]
    fn parser_command_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut parser = Parser::new(&mut lexer);

        let option = block_on(parser.command()).unwrap().unwrap();
        assert_eq!(option, None);
    }

    // TODO test command for other cases

    #[test]
    fn parser_pipeline_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut parser = Parser::new(&mut lexer);

        let option = block_on(parser.pipeline()).unwrap().unwrap();
        assert_eq!(option, None);
    }

    #[test]
    fn parser_pipeline_one() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo");
        let mut parser = Parser::new(&mut lexer);

        let p = block_on(parser.pipeline()).unwrap().unwrap().unwrap();
        let p = p.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(p.negation, false);
        assert_eq!(p.commands.len(), 1);
        assert_eq!(p.commands[0].to_string(), "foo");
    }

    #[test]
    fn parser_pipeline_many() {
        let mut lexer = Lexer::with_source(Source::Unknown, "one | two | \n\t\n three");
        let mut parser = Parser::new(&mut lexer);

        let p = block_on(parser.pipeline()).unwrap().unwrap().unwrap();
        let p = p.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(p.negation, false);
        assert_eq!(p.commands.len(), 3);
        assert_eq!(p.commands[0].to_string(), "one");
        assert_eq!(p.commands[1].to_string(), "two");
        assert_eq!(p.commands[2].to_string(), "three");
    }

    #[test]
    fn parser_pipeline_negated() {
        let mut lexer = Lexer::with_source(Source::Unknown, "! foo");
        let mut parser = Parser::new(&mut lexer);

        let p = block_on(parser.pipeline()).unwrap().unwrap().unwrap();
        let p = p.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(p.negation, true);
        assert_eq!(p.commands.len(), 1);
        assert_eq!(p.commands[0].to_string(), "foo");
    }

    #[test]
    fn parser_pipeline_double_negation() {
        let mut lexer = Lexer::with_source(Source::Unknown, " !  !");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.pipeline()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::DoubleNegation);
        assert_eq!(e.location.line.value, " !  !");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 2);
    }

    #[test]
    fn parser_pipeline_missing_command_after_negation() {
        let mut lexer = Lexer::with_source(Source::Unknown, "!\nfoo");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.pipeline()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::MissingCommandAfterBang);
        assert_eq!(e.location.line.value, "!\n");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }

    #[test]
    fn parser_pipeline_missing_command_after_bar() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo | ;");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.pipeline()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::MissingCommandAfterBar);
        assert_eq!(e.location.line.value, "foo | ;");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 5);
    }

    #[test]
    fn parser_pipeline_bang_after_bar() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo | !");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.pipeline()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::BangAfterBar);
        assert_eq!(e.location.line.value, "foo | !");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 7);
    }

    #[test]
    fn parser_and_or_list_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut parser = Parser::new(&mut lexer);

        let option = block_on(parser.and_or_list()).unwrap().unwrap();
        assert_eq!(option, None);
    }

    #[test]
    fn parser_and_or_list_one() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo");
        let mut parser = Parser::new(&mut lexer);

        let aol = block_on(parser.and_or_list()).unwrap().unwrap().unwrap();
        let aol = aol.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(aol.first.to_string(), "foo");
        assert_eq!(aol.rest, vec![]);
    }

    #[test]
    fn parser_and_or_list_many() {
        let mut lexer = Lexer::with_source(Source::Unknown, "first && second || \n\n third;");
        let mut parser = Parser::new(&mut lexer);

        let aol = block_on(parser.and_or_list()).unwrap().unwrap().unwrap();
        let aol = aol.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(aol.first.to_string(), "first");
        assert_eq!(aol.rest.len(), 2);
        assert_eq!(aol.rest[0].0, AndOr::AndThen);
        assert_eq!(aol.rest[0].1.to_string(), "second");
        assert_eq!(aol.rest[1].0, AndOr::OrElse);
        assert_eq!(aol.rest[1].1.to_string(), "third");
    }

    #[test]
    fn parser_and_or_list_missing_command_after_and_and() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo &&");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.and_or_list()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::MissingPipeline(AndOr::AndThen));
        assert_eq!(e.location.line.value, "foo &&");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 7);
    }

    #[test]
    fn parser_list_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.list()).unwrap().unwrap();
        assert_eq!(list.items, vec![]);
    }

    #[test]
    fn parser_list_one_item_without_last_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.list()).unwrap().unwrap();
        let list = list.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].is_async, false);
        assert_eq!(list.items[0].and_or.to_string(), "foo");
    }

    #[test]
    fn parser_list_one_item_with_last_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo;");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.list()).unwrap().unwrap();
        let list = list.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].is_async, false);
        assert_eq!(list.items[0].and_or.to_string(), "foo");
    }

    #[test]
    fn parser_list_many_items() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo & bar ; baz&");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.list()).unwrap().unwrap();
        let list = list.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(list.items.len(), 3);
        assert_eq!(list.items[0].is_async, true);
        assert_eq!(list.items[0].and_or.to_string(), "foo");
        assert_eq!(list.items[1].is_async, false);
        assert_eq!(list.items[1].and_or.to_string(), "bar");
        assert_eq!(list.items[2].is_async, true);
        assert_eq!(list.items[2].and_or.to_string(), "baz");
    }

    #[test]
    fn parser_command_line_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.command_line()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parser_command_line_command_and_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<END\nfoo\nEND\n");
        let mut parser = Parser::new(&mut lexer);

        let List { items } = block_on(parser.command_line()).unwrap().unwrap();
        assert_eq!(items.len(), 1);
        let item = items.first().unwrap();
        assert_eq!(item.is_async, false);
        let AndOrList { first, rest } = &item.and_or;
        assert!(rest.is_empty(), "expected empty rest: {:?}", rest);
        let Pipeline { commands, negation } = first;
        assert_eq!(*negation, false);
        assert_eq!(commands.len(), 1);
        let cmd = match *commands[0] {
            Command::SimpleCommand(ref c) => c,
        };
        assert_eq!(cmd.words.len(), 0);
        assert_eq!(cmd.redirs.len(), 1);
        assert_eq!(cmd.redirs[0].fd, None);
        let RedirBody::HereDoc(ref here_doc) = cmd.redirs[0].body;
        //if let RedirBody::HereDoc(ref here_doc) = cmd.redirs[0].body {
        let HereDoc {
            delimiter,
            remove_tabs,
            content,
        } = here_doc;
        assert_eq!(delimiter.to_string(), "END");
        assert_eq!(*remove_tabs, false);
        assert_eq!(content.to_string(), "foo\n");
        //} else {
        //panic!("Expected here-document, but got {:?}", cmd.redirs[0].body);
        //}
    }

    #[test]
    fn parser_command_line_command_without_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo");
        let mut parser = Parser::new(&mut lexer);

        let cmd = block_on(parser.command_line()).unwrap().unwrap();
        assert_eq!(cmd.to_string(), "foo");
    }

    #[test]
    fn parser_command_line_newline_only() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\n");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.command_line()).unwrap().unwrap();
        assert_eq!(list.items.len(), 0);
    }

    #[test]
    fn parser_command_line_here_doc_without_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<END");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.command_line()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::MissingHereDocContent);
        assert_eq!(e.location.line.value, "<<END");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 3);
    }

    #[test]
    fn parser_command_line_wrong_delimiter() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo)");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.command_line()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::UnexpectedToken);
        assert_eq!(e.location.line.value, "foo)");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 4);
    }
}
