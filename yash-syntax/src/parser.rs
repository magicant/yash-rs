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
mod fromstr;

mod case;
mod command;
mod compound_command;
mod for_loop;
mod function;
mod grouping;
mod redir;
mod simple_command;
mod while_loop;

pub mod lex;

use self::lex::keyword::Keyword::*;
use self::lex::Operator::*;
use self::lex::TokenId::{EndOfInput, Operator, Token};
use super::syntax::*;
use std::future::Future;
use std::pin::Pin;

pub use self::core::AsyncFnMut;
pub use self::core::Error;
pub use self::core::Parser;
pub use self::core::Rec;
pub use self::core::Result;
pub use self::core::SyntaxError;
pub use self::fill::Fill;
pub use self::fill::MissingHereDoc;

impl Parser<'_> {
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
                if let Token(Some(Bang)) = self.peek_token().await?.id {
                    let location = self.take_token_raw().await?.word.location;
                    loop {
                        // Parse the command after the `!`
                        if let Rec::Parsed(option) = self.command().await? {
                            if let Some(first) = option {
                                break (first, true);
                            }

                            // Error: the command is missing
                            let next = self.peek_token().await?;
                            let cause = if next.id == Token(Some(Bang)) {
                                SyntaxError::DoubleNegation.into()
                            } else {
                                SyntaxError::MissingCommandAfterBang.into()
                            };
                            return Err(Error { cause, location });
                        }
                    }
                } else {
                    return Ok(Rec::Parsed(None));
                }
            }
        };

        // Parse `|`
        let mut commands = vec![first];
        while self.peek_token().await?.id == Operator(Bar) {
            let bar_location = self.take_token_raw().await?.word.location;

            while self.newline_and_here_doc_contents().await? {}

            commands.push(loop {
                // Parse the next command
                if let Rec::Parsed(option) = self.command().await? {
                    if let Some(next) = option {
                        break next;
                    }

                    // Error: the command is missing
                    let next = self.peek_token().await?;
                    return if next.id == Token(Some(Bang)) {
                        Err(Error {
                            cause: SyntaxError::BangAfterBar.into(),
                            location: next.word.location.clone(),
                        })
                    } else {
                        Err(Error {
                            cause: SyntaxError::MissingCommandAfterBar.into(),
                            location: bar_location,
                        })
                    };
                }
            });
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
            self.take_token_raw().await?;

            while self.newline_and_here_doc_contents().await? {}

            let maybe_pipeline = loop {
                if let Rec::Parsed(maybe_pipeline) = self.pipeline().await? {
                    break maybe_pipeline;
                }
            };
            let pipeline = match maybe_pipeline {
                None => {
                    let cause = SyntaxError::MissingPipeline(condition).into();
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

        let mut result = match self.and_or_list().await? {
            Rec::AliasSubstituted => return Ok(Rec::AliasSubstituted),
            Rec::Parsed(result) => result,
        };

        while let Some(and_or) = result {
            let (is_async, next) = match self.peek_token().await?.id {
                Operator(Semicolon) => (false, true),
                Operator(And) => (true, true),
                _ => (false, false),
            };

            items.push(Item { and_or, is_async });

            if !next {
                break;
            }
            self.take_token_raw().await?;

            result = loop {
                if let Rec::Parsed(result) = self.and_or_list().await? {
                    break result;
                }
            };
        }

        Ok(Rec::Parsed(List(items)))
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

        self.take_token_raw().await?;
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
                    cause: SyntaxError::UnexpectedToken.into(),
                    location: next.word.location.clone(),
                });
            }
            if list.0.is_empty() {
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
            items.extend(list.0);

            if !self.newline_and_here_doc_contents().await? {
                break;
            }
        }

        Ok(List(items))
    }

    /// Like [`maybe_compound_list`](Self::maybe_compound_list), but returns the future in a pinned box.
    pub fn maybe_compound_list_boxed(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<List<MissingHereDoc>>> + '_>> {
        Box::pin(self.maybe_compound_list())
    }
}

#[cfg(test)]
mod tests {
    use super::core::ErrorCause;
    use super::lex::Lexer;
    use super::*;
    use crate::alias::{AliasSet, HashEntry};
    use crate::source::{Location, Source};
    use futures::executor::block_on;

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
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::DoubleNegation));
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
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingCommandAfterBang)
        );
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
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingCommandAfterBar)
        );
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
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::BangAfterBar));
        assert_eq!(e.location.line.value, "foo | !");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 7);
    }

    #[test]
    fn parser_pipeline_no_aliasing_of_bang() {
        let mut lexer = Lexer::with_source(Source::Unknown, "! ok");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("".to_string());
        aliases.insert(HashEntry::new(
            "!".to_string(),
            "; ; ;".to_string(),
            true,
            origin,
        ));
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let p = block_on(parser.pipeline()).unwrap().unwrap().unwrap();
        let p = p.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(p.negation, true);
        assert_eq!(p.commands.len(), 1);
        assert_eq!(p.commands[0].to_string(), "ok");
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
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingPipeline(AndOr::AndThen))
        );
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
        assert_eq!(list.0, vec![]);
    }

    #[test]
    fn parser_list_one_item_without_last_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.list()).unwrap().unwrap();
        let list = list.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(list.0.len(), 1);
        assert_eq!(list.0[0].is_async, false);
        assert_eq!(list.0[0].and_or.to_string(), "foo");
    }

    #[test]
    fn parser_list_one_item_with_last_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo;");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.list()).unwrap().unwrap();
        let list = list.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(list.0.len(), 1);
        assert_eq!(list.0[0].is_async, false);
        assert_eq!(list.0[0].and_or.to_string(), "foo");
    }

    #[test]
    fn parser_list_many_items() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo & bar ; baz&");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.list()).unwrap().unwrap();
        let list = list.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(list.0.len(), 3);
        assert_eq!(list.0[0].is_async, true);
        assert_eq!(list.0[0].and_or.to_string(), "foo");
        assert_eq!(list.0[1].is_async, false);
        assert_eq!(list.0[1].and_or.to_string(), "bar");
        assert_eq!(list.0[2].is_async, true);
        assert_eq!(list.0[2].and_or.to_string(), "baz");
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

        let List(items) = block_on(parser.command_line()).unwrap().unwrap();
        assert_eq!(items.len(), 1);
        let item = items.first().unwrap();
        assert_eq!(item.is_async, false);
        let AndOrList { first, rest } = &item.and_or;
        assert!(rest.is_empty(), "expected empty rest: {:?}", rest);
        let Pipeline { commands, negation } = first;
        assert_eq!(*negation, false);
        assert_eq!(commands.len(), 1);
        let cmd = match commands[0] {
            Command::Simple(ref c) => c,
            _ => panic!("Expected a simple command but got {:?}", commands[0]),
        };
        assert_eq!(cmd.words, []);
        assert_eq!(cmd.redirs.len(), 1);
        assert_eq!(cmd.redirs[0].fd, None);
        if let RedirBody::HereDoc(ref here_doc) = cmd.redirs[0].body {
            let HereDoc {
                delimiter,
                remove_tabs,
                content,
            } = here_doc;
            assert_eq!(delimiter.to_string(), "END");
            assert_eq!(*remove_tabs, false);
            assert_eq!(content.to_string(), "foo\n");
        } else {
            panic!("Expected here-document, but got {:?}", cmd.redirs[0].body);
        }
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
        assert_eq!(list.0, []);
    }

    #[test]
    fn parser_command_line_here_doc_without_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<END");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.command_line()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
        );
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
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::UnexpectedToken));
        assert_eq!(e.location.line.value, "foo)");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 4);
    }
}
