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

//! Syntax parser for list and compound list

use super::core::Parser;
use super::core::Rec;
use super::core::Result;
use super::error::Error;
use super::error::SyntaxError;
use super::lex::Operator::{And, Newline, Semicolon};
use super::lex::TokenId::{self, EndOfInput, IoNumber, Operator, Token};
use crate::syntax::Item;
use crate::syntax::List;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

fn error_type_for_trailing_token_in_command_line(token_id: TokenId) -> Option<SyntaxError> {
    use super::lex::Keyword::*;
    use super::lex::Operator::*;
    use SyntaxError::*;
    match token_id {
        EndOfInput => None,
        Token(None) | IoNumber => Some(MissingSeparator),
        Token(Some(keyword)) => match keyword {
            Bang | OpenBracketBracket | Case | For | Function | If | Until | While | OpenBrace => {
                Some(MissingSeparator)
            }
            Do => Some(UnopenedLoop),
            Done => Some(UnopenedDoClause),
            Elif | Else | Fi | Then => Some(UnopenedIf),
            Esac => Some(UnopenedCase),
            In => Some(InAsCommandName),
            CloseBrace => Some(UnopenedGrouping),
        },
        Operator(operator) => match operator {
            And | AndAnd | Semicolon | Bar | BarBar => Some(InvalidCommandToken),
            OpenParen => Some(MissingSeparator),
            CloseParen => Some(UnopenedSubshell),
            SemicolonAnd | SemicolonSemicolon | SemicolonSemicolonAnd | SemicolonBar => {
                Some(UnopenedCase)
            }
            Newline | Less | LessAnd | LessOpenParen | LessLess | LessLessDash | LessLessLess
            | LessGreater | Greater | GreaterAnd | GreaterOpenParen | GreaterGreater
            | GreaterGreaterBar | GreaterBar => unreachable!(),
        },
    }
}

impl Parser<'_, '_> {
    // There is no function that parses a single item because it would not be
    // very useful for parsing a list. An item requires a separator operator
    // ('&' or ';') for it to be followed by another item. You cannot tell from
    // the resultant item whether there was a separator operator.
    // pub async fn item(&mut self) -> Result<Rec<Item>> { }

    /// Parses a list.
    ///
    /// This function parses a sequence of and-or lists that are separated by `;`
    /// or `&`. A newline token that delimits the list is not parsed.
    ///
    /// If there is no valid command at the current position, this function
    /// returns a list with no items.
    pub async fn list(&mut self) -> Result<Rec<List>> {
        let mut items = vec![];

        let mut result = match self.and_or_list().await? {
            Rec::AliasSubstituted => return Ok(Rec::AliasSubstituted),
            Rec::Parsed(result) => result,
        };

        while let Some(and_or) = result {
            let token = self.peek_token().await?;
            let (async_flag, next) = match token.id {
                Operator(Semicolon) => (None, true),
                Operator(And) => (Some(token.word.location.clone()), true),
                _ => (None, false),
            };

            let and_or = Rc::new(and_or);
            items.push(Item { and_or, async_flag });

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

    // TODO Consider returning Result<Result<(), &Token>, Error>
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
            if let Some(syntax_error) = error_type_for_trailing_token_in_command_line(next.id) {
                let cause = syntax_error.into();
                let location = next.word.location.clone();
                return Err(Error { cause, location });
            }
            if list.0.is_empty() {
                return Ok(None);
            }
        }

        self.ensure_no_unread_here_doc()?;
        Ok(Some(list))
    }

    /// Parses an optional compound list.
    ///
    /// A compound list is a sequence of one or more and-or lists that are
    /// separated by newlines and optionally preceded and/or followed by
    /// newlines.
    ///
    /// This function stops parsing on encountering an unexpected token that
    /// cannot be parsed as the beginning of an and-or list. If the token is a
    /// possible [clause delimiter](super::lex::TokenId::is_clause_delimiter),
    /// the result is a list of commands that have been parsed up to the token.
    /// Otherwise, an `InvalidCommandToken` error is returned.
    pub async fn maybe_compound_list(&mut self) -> Result<List> {
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

        let next = self.peek_token().await?;
        if next.id.is_clause_delimiter() {
            Ok(List(items))
        } else {
            let cause = SyntaxError::InvalidCommandToken.into();
            let location = next.word.location.clone();
            Err(Error { cause, location })
        }
    }

    /// Like [`maybe_compound_list`](Self::maybe_compound_list), but returns the future in a pinning box.
    pub fn maybe_compound_list_boxed(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<List>> + '_>> {
        Box::pin(self.maybe_compound_list())
    }
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::lex::Lexer;
    use super::*;
    use crate::source::Source;
    use crate::syntax::AndOrList;
    use crate::syntax::Command;
    use crate::syntax::Pipeline;
    use crate::syntax::RedirBody;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn parser_list_eof() {
        let mut lexer = Lexer::with_code("");
        let mut parser = Parser::new(&mut lexer);

        let list = parser.list().now_or_never().unwrap().unwrap().unwrap();
        assert_eq!(list.0, vec![]);
    }

    #[test]
    fn parser_list_one_item_without_last_semicolon() {
        let mut lexer = Lexer::with_code("foo");
        let mut parser = Parser::new(&mut lexer);

        let list = parser.list().now_or_never().unwrap().unwrap().unwrap();
        assert_eq!(list.0.len(), 1);
        assert_eq!(list.0[0].async_flag, None);
        assert_eq!(list.0[0].and_or.to_string(), "foo");
    }

    #[test]
    fn parser_list_one_item_with_last_semicolon() {
        let mut lexer = Lexer::with_code("foo;");
        let mut parser = Parser::new(&mut lexer);

        let list = parser.list().now_or_never().unwrap().unwrap().unwrap();
        assert_eq!(list.0.len(), 1);
        assert_eq!(list.0[0].async_flag, None);
        assert_eq!(list.0[0].and_or.to_string(), "foo");
    }

    #[test]
    fn parser_list_many_items() {
        let mut lexer = Lexer::with_code("foo & bar ; baz&");
        let mut parser = Parser::new(&mut lexer);

        let list = parser.list().now_or_never().unwrap().unwrap().unwrap();
        assert_eq!(list.0.len(), 3);

        let location = list.0[0].async_flag.as_ref().unwrap();
        assert_eq!(*location.code.value.borrow(), "foo & bar ; baz&");
        assert_eq!(location.code.start_line_number.get(), 1);
        assert_eq!(*location.code.source, Source::Unknown);
        assert_eq!(location.range, 4..5);
        assert_eq!(list.0[0].and_or.to_string(), "foo");

        assert_eq!(list.0[1].async_flag, None);
        assert_eq!(list.0[1].and_or.to_string(), "bar");

        let location = list.0[2].async_flag.as_ref().unwrap();
        assert_eq!(*location.code.value.borrow(), "foo & bar ; baz&");
        assert_eq!(location.code.start_line_number.get(), 1);
        assert_eq!(*location.code.source, Source::Unknown);
        assert_eq!(location.range, 15..16);
        assert_eq!(list.0[2].and_or.to_string(), "baz");
    }

    #[test]
    fn parser_command_line_eof() {
        let mut lexer = Lexer::with_code("");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.command_line().now_or_never().unwrap().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parser_command_line_command_and_newline() {
        let mut lexer = Lexer::with_code("<<END\nfoo\nEND\n");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.command_line().now_or_never().unwrap();
        let List(items) = result.unwrap().unwrap();
        assert_eq!(items.len(), 1);
        let item = items.first().unwrap();
        assert_eq!(item.async_flag, None);
        let AndOrList { first, rest } = &*item.and_or;
        assert!(rest.is_empty(), "expected empty rest: {rest:?}");
        let Pipeline { commands, negation } = first;
        assert_eq!(*negation, false);
        assert_eq!(commands.len(), 1);
        let cmd = assert_matches!(*commands[0], Command::Simple(ref c) => c);
        assert_eq!(cmd.words, []);
        assert_eq!(cmd.redirs.len(), 1);
        assert_eq!(cmd.redirs[0].fd, None);
        assert_matches!(cmd.redirs[0].body, RedirBody::HereDoc(ref here_doc) => {
            assert_eq!(here_doc.delimiter.to_string(), "END");
            assert_eq!(here_doc.remove_tabs, false);
            assert_eq!(here_doc.content.get().unwrap().to_string(), "foo\n");
        });
    }

    #[test]
    fn parser_command_line_command_without_newline() {
        let mut lexer = Lexer::with_code("foo");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.command_line().now_or_never().unwrap();
        let list = result.unwrap().unwrap();
        assert_eq!(list.to_string(), "foo");
    }

    #[test]
    fn parser_command_line_newline_only() {
        let mut lexer = Lexer::with_code("\n");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.command_line().now_or_never().unwrap();
        let list = result.unwrap().unwrap();
        assert_eq!(list.0, []);
    }

    #[test]
    fn parser_command_line_here_doc_without_newline() {
        let mut lexer = Lexer::with_code("<<END");
        let mut parser = Parser::new(&mut lexer);

        let e = parser.command_line().now_or_never().unwrap().unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
        );
        assert_eq!(*e.location.code.value.borrow(), "<<END");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 2..5);
    }

    #[test]
    fn parser_command_line_wrong_delimiter_1() {
        let mut lexer = Lexer::with_code("foo)");
        let mut parser = Parser::new(&mut lexer);

        let e = parser.command_line().now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::UnopenedSubshell));
        assert_eq!(*e.location.code.value.borrow(), "foo)");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 3..4);
    }

    #[test]
    fn parser_command_line_wrong_delimiter_2() {
        let mut lexer = Lexer::with_code("foo bar (");
        let mut parser = Parser::new(&mut lexer);

        let e = parser.command_line().now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MissingSeparator));
        assert_eq!(*e.location.code.value.borrow(), "foo bar (");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 8..9);
    }

    #[test]
    fn parser_command_line_wrong_delimiter_3() {
        let mut lexer = Lexer::with_code("foo bar; ;");
        let mut parser = Parser::new(&mut lexer);

        let e = parser.command_line().now_or_never().unwrap().unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::InvalidCommandToken)
        );
        assert_eq!(*e.location.code.value.borrow(), "foo bar; ;");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 9..10);
    }

    #[test]
    fn parser_maybe_compound_list_empty() {
        let mut lexer = Lexer::with_code("");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.maybe_compound_list().now_or_never().unwrap();
        let list = result.unwrap();
        assert_eq!(list.0, []);
    }

    #[test]
    fn parser_maybe_compound_list_some_commands() {
        let mut lexer = Lexer::with_code("echo; ls& cat");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.maybe_compound_list().now_or_never().unwrap();
        let list = result.unwrap();
        assert_eq!(list.to_string(), "echo; ls& cat");
    }

    #[test]
    fn parser_maybe_compound_list_some_commands_with_newline() {
        let mut lexer = Lexer::with_code("echo& ls\n\ncat\n\n");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.maybe_compound_list().now_or_never().unwrap();
        let list = result.unwrap();
        assert_eq!(list.to_string(), "echo& ls; cat");

        assert_eq!(lexer.index(), 15);
    }

    #[test]
    fn parser_maybe_compound_list_empty_with_delimiter() {
        let mut lexer = Lexer::with_code("}");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.maybe_compound_list().now_or_never().unwrap();
        let list = result.unwrap();
        assert_eq!(list.0, []);
    }

    // TODO Test maybe_compound_list with alias substitution

    #[test]
    fn parser_maybe_compound_list_empty_with_invalid_delimiter() {
        let mut lexer = Lexer::with_code(";");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.maybe_compound_list().now_or_never().unwrap();
        let e = result.unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::InvalidCommandToken)
        );
        assert_eq!(*e.location.code.value.borrow(), ";");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 0..1);
    }

    #[test]
    fn parser_maybe_compound_list_some_commands_with_invalid_delimiter() {
        let mut lexer = Lexer::with_code("echo; ls\n &");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.maybe_compound_list().now_or_never().unwrap();
        let e = result.unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::InvalidCommandToken)
        );
        assert_eq!(*e.location.code.value.borrow(), "echo; ls\n &");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 10..11);
    }
}
