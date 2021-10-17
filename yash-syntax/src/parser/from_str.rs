// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

use super::fill::Fill;
use super::fill::MissingHereDoc;
use super::lex::Lexer;
use super::lex::Operator;
use super::lex::TokenId;
use super::lex::WordContext;
use super::lex::WordLexer;
use super::Error;
use super::Parser;
use super::Rec;
use crate::source::Source;
use crate::syntax::*;
use std::convert::TryInto;
use std::future::Future;
use std::iter::empty;
use std::str::FromStr;

// TODO Most FromStr implementations in this file ignore trailing redundant
// tokens, which should be rejected.

/// Polls the given future, assuming it returns `Ready`.
fn unwrap_ready<F: Future>(f: F) -> <F as Future>::Output {
    use futures_util::future::FutureExt;
    f.now_or_never()
        .expect("Expected Ready but received Pending")
}

/// Helper for implementing FromStr.
trait Shift {
    type Output;
    fn shift(self) -> Self::Output;
}

impl<T, E> Shift for Result<Option<T>, E> {
    type Output = Result<T, Option<E>>;
    fn shift(self) -> Result<T, Option<E>> {
        match self {
            Ok(Some(t)) => Ok(t),
            Ok(None) => Err(None),
            Err(e) => Err(Some(e)),
        }
    }
}

impl FromStr for Param {
    type Err = Option<Error>;
    fn from_str(s: &str) -> Result<Param, Option<Error>> {
        match TextUnit::from_str(s) {
            Err(e) => Err(Some(e)),
            Ok(TextUnit::BracedParam(param)) => Ok(param),
            Ok(_) => Err(None),
        }
    }
}

impl FromStr for TextUnit {
    type Err = Error;
    /// Parses a [`TextUnit`] by `lexer.text_unit(|_| false, |_| true)`.
    fn from_str(s: &str) -> Result<TextUnit, Error> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        unwrap_ready(lexer.text_unit(|_| false, |_| true)).map(Option::unwrap)
    }
}

impl FromStr for Text {
    type Err = Error;
    // Parses a text by `lexer.text(|_| false, |_| true)`.
    fn from_str(s: &str) -> Result<Text, Error> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        unwrap_ready(lexer.text(|_| false, |_| true))
    }
}

impl FromStr for WordUnit {
    type Err = Error;
    fn from_str(s: &str) -> Result<WordUnit, Error> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        unwrap_ready(lexer.word_unit(|_| false)).map(Option::unwrap)
    }
}

impl FromStr for Word {
    type Err = Error;

    /// Converts a string to a word.
    ///
    /// This function does not parse any tilde expansions in the word.
    /// To parse them, you need to call [`Word::parse_tilde_front`] or
    /// [`Word::parse_tilde_everywhere`] on the resultant word.
    fn from_str(s: &str) -> Result<Word, Error> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        unwrap_ready(lexer.word(|_| false))
    }
}

impl FromStr for Value {
    type Err = Error;
    fn from_str(s: &str) -> Result<Value, Error> {
        let s = format!("x={}", s);
        let a = Assign::from_str(&s).map_err(Option::unwrap)?;
        Ok(a.value)
    }
}

impl FromStr for Assign {
    type Err = Option<Error>;
    /// Converts a string to an assignment.
    ///
    /// Returns `Err(None)` if the string is not an assignment word.
    fn from_str(s: &str) -> Result<Assign, Option<Error>> {
        let c: SimpleCommand<MissingHereDoc> = s.parse()?;
        Ok(c.assigns.into_iter().next()).shift()
    }
}

impl FromStr for Operator {
    type Err = ();
    fn from_str(s: &str) -> Result<Operator, ()> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let token = unwrap_ready(lexer.operator()).map_err(drop)?.ok_or(())?;
        if let TokenId::Operator(op) = token.id {
            Ok(op)
        } else {
            Err(())
        }
    }
}

impl FromStr for RedirOp {
    type Err = ();
    fn from_str(s: &str) -> Result<RedirOp, ()> {
        Operator::from_str(s)?.try_into()
    }
}

impl FromStr for Redir<MissingHereDoc> {
    type Err = Option<Error>;
    /// Converts a string to a redirection.
    ///
    /// Returns `Err(None)` if the first token is not a redirection operator.
    fn from_str(s: &str) -> Result<Redir<MissingHereDoc>, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        unwrap_ready(parser.redirection()).shift()
    }
}

impl FromStr for Redir {
    type Err = Option<Error>;
    /// Converts a string to a redirection.
    ///
    /// Returns `Err(None)` if the first token is not a redirection operator.
    ///
    /// This function does not support parsing a here-document.
    fn from_str(s: &str) -> Result<Redir, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        let redir = unwrap_ready(parser.redirection()).shift()?;
        parser.ensure_no_unread_here_doc()?;
        // If this redirection is a here-document, its content cannot be filled
        // because there is no newline token that would make the content to be
        // read.
        redir.fill(&mut empty()).map_err(Some)
    }
}

impl FromStr for SimpleCommand<MissingHereDoc> {
    type Err = Option<Error>;
    /// Converts a string to a simple command.
    ///
    /// Returns `Err(None)` if the first token does not start a simple command.
    fn from_str(s: &str) -> Result<SimpleCommand<MissingHereDoc>, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        unwrap_ready(parser.simple_command())
            .map(Rec::unwrap)
            .shift()
    }
}

impl FromStr for SimpleCommand {
    type Err = Option<Error>;
    /// Converts a string to a simple command.
    ///
    /// Returns `Err(None)` if the first token does not start a simple command.
    ///
    /// This function does not support parsing a command that contains a
    /// here-document.
    fn from_str(s: &str) -> Result<SimpleCommand, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        let command = unwrap_ready(parser.simple_command())
            .map(Rec::unwrap)
            .shift()?;
        parser.ensure_no_unread_here_doc()?;
        // If the simple command contains a here-document, its content cannot be
        // filled because there is no newline token that would make the content
        // to be read.
        command.fill(&mut empty()).map_err(Some)
    }
}

impl FromStr for CaseItem<MissingHereDoc> {
    type Err = Option<Error>;
    /// Converts a string to a case item.
    ///
    /// Returns `Err(None)` if the first token is `esac`.
    fn from_str(s: &str) -> Result<CaseItem<MissingHereDoc>, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        unwrap_ready(parser.case_item()).shift()
    }
}

impl FromStr for CaseItem {
    type Err = Option<Error>;
    /// Converts a string to a case item.
    ///
    /// Returns `Err(None)` if the first token is `esac`.
    fn from_str(s: &str) -> Result<CaseItem, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        let item = unwrap_ready(parser.case_item()).shift()?;
        parser.ensure_no_unread_here_doc()?;
        let mut here_docs = parser.take_read_here_docs().into_iter();
        item.fill(&mut here_docs).map_err(Some)
    }
}

impl FromStr for CompoundCommand<MissingHereDoc> {
    type Err = Option<Error>;
    /// Converts a string to a compound command.
    ///
    /// Returns `Err(None)` if the first token does not start a compound command.
    fn from_str(s: &str) -> Result<CompoundCommand<MissingHereDoc>, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        unwrap_ready(parser.compound_command()).shift()
    }
}

impl FromStr for CompoundCommand {
    type Err = Option<Error>;
    /// Converts a string to a compound command.
    ///
    /// Returns `Err(None)` if the first token does not start a compound command.
    fn from_str(s: &str) -> Result<CompoundCommand, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        let command = unwrap_ready(parser.compound_command()).shift()?;
        parser.ensure_no_unread_here_doc()?;
        let mut here_docs = parser.take_read_here_docs().into_iter();
        command.fill(&mut here_docs).map_err(Some)
    }
}

impl FromStr for FullCompoundCommand<MissingHereDoc> {
    type Err = Option<Error>;
    /// Converts a string to a compound command.
    ///
    /// Returns `Err(None)` if the first token does not start a compound command.
    fn from_str(s: &str) -> Result<FullCompoundCommand<MissingHereDoc>, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        unwrap_ready(parser.full_compound_command()).shift()
    }
}

impl FromStr for FullCompoundCommand {
    type Err = Option<Error>;
    /// Converts a string to a compound command.
    ///
    /// Returns `Err(None)` if the first token does not start a compound command.
    fn from_str(s: &str) -> Result<FullCompoundCommand, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        let command = unwrap_ready(parser.full_compound_command()).shift()?;
        parser.ensure_no_unread_here_doc()?;
        let mut here_docs = parser.take_read_here_docs().into_iter();
        command.fill(&mut here_docs).map_err(Some)
    }
}

impl FromStr for Command<MissingHereDoc> {
    type Err = Option<Error>;
    /// Converts a string to a command.
    ///
    /// Returns `Err(None)` if the first token does not start a command.
    fn from_str(s: &str) -> Result<Command<MissingHereDoc>, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        unwrap_ready(parser.command()).map(Rec::unwrap).shift()
    }
}

impl FromStr for Command {
    type Err = Option<Error>;
    /// Converts a string to a command.
    ///
    /// Returns `Err(None)` if the first token does not start a command.
    fn from_str(s: &str) -> Result<Command, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        let command = unwrap_ready(parser.command()).map(Rec::unwrap).shift()?;
        parser.ensure_no_unread_here_doc()?;
        let mut here_docs = parser.take_read_here_docs().into_iter();
        command.fill(&mut here_docs).map_err(Some)
    }
}

impl FromStr for Pipeline<MissingHereDoc> {
    type Err = Option<Error>;
    /// Converts a string to a pipeline.
    ///
    /// Returns `Err(None)` if the first token does not start a pipeline.
    fn from_str(s: &str) -> Result<Pipeline<MissingHereDoc>, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        unwrap_ready(parser.pipeline()).map(Rec::unwrap).shift()
    }
}

impl FromStr for Pipeline {
    type Err = Option<Error>;
    /// Converts a string to a pipeline.
    ///
    /// Returns `Err(None)` if the first token does not start a pipeline.
    fn from_str(s: &str) -> Result<Pipeline, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        let pipeline = unwrap_ready(parser.pipeline()).map(Rec::unwrap).shift()?;
        parser.ensure_no_unread_here_doc()?;
        let mut here_docs = parser.take_read_here_docs().into_iter();
        pipeline.fill(&mut here_docs).map_err(Some)
    }
}

impl FromStr for AndOr {
    type Err = ();
    fn from_str(s: &str) -> Result<AndOr, ()> {
        Operator::from_str(s)?.try_into()
    }
}

impl FromStr for AndOrList<MissingHereDoc> {
    type Err = Option<Error>;
    /// Converts a string to an and-or list.
    ///
    /// Returns `Err(None)` if the first token does not start an and-or list.
    fn from_str(s: &str) -> Result<AndOrList<MissingHereDoc>, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        unwrap_ready(parser.and_or_list()).map(Rec::unwrap).shift()
    }
}

impl FromStr for AndOrList {
    type Err = Option<Error>;
    /// Converts a string to an and-or list.
    ///
    /// Returns `Err(None)` if the first token does not start an and-or list.
    fn from_str(s: &str) -> Result<AndOrList, Option<Error>> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        let list = unwrap_ready(parser.and_or_list())
            .map(Rec::unwrap)
            .shift()?;
        parser.ensure_no_unread_here_doc()?;
        let mut here_docs = parser.take_read_here_docs().into_iter();
        list.fill(&mut here_docs).map_err(Some)
    }
}

impl FromStr for List<MissingHereDoc> {
    type Err = Error;
    /// Converts a string to a list.
    ///
    /// Returns `Err(None)` if the first token does not start a list.
    fn from_str(s: &str) -> Result<List<MissingHereDoc>, Error> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        unwrap_ready(parser.list()).map(Rec::unwrap)
    }
}

impl FromStr for List {
    type Err = Error;
    /// Converts a string to a list.
    ///
    /// Returns `Err(None)` if the first token does not start a list.
    fn from_str(s: &str) -> Result<List, Error> {
        let mut lexer = Lexer::with_source(Source::Unknown, s);
        let mut parser = Parser::new(&mut lexer);
        let list = unwrap_ready(parser.maybe_compound_list())?;
        parser.ensure_no_unread_here_doc()?;
        let mut here_docs = parser.take_read_here_docs().into_iter();
        list.fill(&mut here_docs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ErrorCause;
    use crate::parser::SyntaxError;
    use futures_executor::block_on;

    // Most of the tests below are surrounded with `block_on(async {...})` in
    // order to make sure `str::parse` can be called in an executor context.

    #[test]
    fn param_from_str() {
        block_on(async {
            let parse: Param = "${foo}".parse().unwrap();
            assert_eq!(parse.to_string(), "${foo}");
        })
    }

    #[test]
    fn text_unit_from_str() {
        block_on(async {
            let parse: TextUnit = "a".parse().unwrap();
            assert_eq!(parse.to_string(), "a");
        })
    }

    #[test]
    fn text_from_str() {
        block_on(async {
            let parse: Text = r"a\b$(c)".parse().unwrap();
            assert_eq!(parse.0.len(), 3);
            assert_eq!(parse.0[0], Literal('a'));
            assert_eq!(parse.0[1], Backslashed('b'));
            if let CommandSubst { content, .. } = &parse.0[2] {
                assert_eq!(content, "c");
            } else {
                panic!("not a command substitution: {:?}", parse.0[2]);
            }
        })
    }

    #[test]
    fn word_unit_from_str() {
        block_on(async {
            let parse: WordUnit = "a".parse().unwrap();
            assert_eq!(parse.to_string(), "a");
        })
    }

    #[test]
    fn word_from_str() {
        block_on(async {
            let parse: Word = "a".parse().unwrap();
            assert_eq!(parse.to_string(), "a");
        })
    }

    #[test]
    fn value_from_str() {
        block_on(async {
            let parse: Value = "v".parse().unwrap();
            assert_eq!(parse.to_string(), "v");

            let parse: Value = "(1 2 3)".parse().unwrap();
            assert_eq!(parse.to_string(), "(1 2 3)");
        })
    }

    #[test]
    fn assign_from_str() {
        block_on(async {
            let parse: Assign = "a=b".parse().unwrap();
            assert_eq!(parse.to_string(), "a=b");

            let parse: Assign = "x=(1 2 3)".parse().unwrap();
            assert_eq!(parse.to_string(), "x=(1 2 3)");
        })
    }

    #[test]
    fn operator_from_str() {
        block_on(async {
            let parse: Operator = "<<".parse().unwrap();
            assert_eq!(parse, Operator::LessLess);
        })
    }

    #[test]
    fn redir_op_from_str() {
        block_on(async {
            let parse: RedirOp = ">|".parse().unwrap();
            assert_eq!(parse, RedirOp::FileClobber);
        })
    }

    #[test]
    fn redir_from_str() {
        block_on(async {
            let parse: Redir<MissingHereDoc> = "2> /dev/null".parse().unwrap();
            assert_eq!(parse.fd, Some(Fd(2)));
            if let RedirBody::Normal { operator, operand } = parse.body {
                assert_eq!(operator, RedirOp::FileOut);
                assert_eq!(operand.to_string(), "/dev/null");
            } else {
                panic!("Not normal redirection: {:?}", parse.body);
            }
        })
    }

    #[test]
    fn redir_from_str_unfillable_here_doc_content() {
        block_on(async {
            let result: Result<Redir, _> = "<<FOO".parse();
            let e = result.unwrap_err().unwrap();
            assert_eq!(
                e.cause,
                ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
            );
        })
    }

    #[test]
    fn simple_command_from_str() {
        block_on(async {
            let parse: SimpleCommand<MissingHereDoc> = " a=b</dev/null foo ".parse().unwrap();
            let parse = parse.fill(&mut empty()).unwrap();
            assert_eq!(parse.to_string(), "a=b foo </dev/null");
        })
    }

    #[test]
    fn simple_command_from_str_unfillable_here_doc_content() {
        block_on(async {
            let result: Result<SimpleCommand, _> = "<<FOO".parse();
            let e = result.unwrap_err().unwrap();
            assert_eq!(
                e.cause,
                ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
            );
        })
    }

    #[test]
    fn case_item_from_str() {
        block_on(async {
            let parse: CaseItem<MissingHereDoc> = " foo) ".parse().unwrap();
            let parse = parse.fill(&mut empty()).unwrap();
            assert_eq!(parse.to_string(), "(foo) ;;");
        })
    }

    #[test]
    fn case_item_from_str_unfillable_here_doc_content() {
        block_on(async {
            let result: Result<CaseItem, _> = "(foo) <<FOO".parse();
            let e = result.unwrap_err().unwrap();
            assert_eq!(
                e.cause,
                ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
            );
        })
    }

    #[test]
    fn compound_command_from_str() {
        block_on(async {
            let parse: CompoundCommand<MissingHereDoc> = " { :; } ".parse().unwrap();
            let parse = parse.fill(&mut empty()).unwrap();
            assert_eq!(parse.to_string(), "{ :; }");
        })
    }

    #[test]
    fn compound_command_from_str_unfillable_here_doc_content() {
        block_on(async {
            let result: Result<CompoundCommand, _> = "{ <<FOO; }".parse();
            let e = result.unwrap_err().unwrap();
            assert_eq!(
                e.cause,
                ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
            );
        })
    }

    #[test]
    fn full_compound_command_from_str() {
        block_on(async {
            let parse: FullCompoundCommand<MissingHereDoc> = " { :; } <&- ".parse().unwrap();
            let parse = parse.fill(&mut empty()).unwrap();
            assert_eq!(parse.to_string(), "{ :; } <&-");
        })
    }

    #[test]
    fn full_compound_command_from_str_unfillable_here_doc_content() {
        block_on(async {
            let result: Result<FullCompoundCommand, _> = "{ :; } <<FOO".parse();
            let e = result.unwrap_err().unwrap();
            assert_eq!(
                e.cause,
                ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
            );
        })
    }

    #[test]
    fn command_from_str() {
        block_on(async {
            let parse: Command<MissingHereDoc> = "f(){ :; }>&2".parse().unwrap();
            let parse = parse.fill(&mut empty()).unwrap();
            assert_eq!(parse.to_string(), "f() { :; } >&2");
        })
    }

    #[test]
    fn command_from_str_unfillable_here_doc_content() {
        block_on(async {
            let result: Result<Command, _> = "<<FOO".parse();
            let e = result.unwrap_err().unwrap();
            assert_eq!(
                e.cause,
                ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
            );
        })
    }

    #[test]
    fn pipeline_from_str() {
        block_on(async {
            let parse: Pipeline<MissingHereDoc> = " ! a|b|c".parse().unwrap();
            let parse = parse.fill(&mut empty()).unwrap();
            assert_eq!(parse.to_string(), "! a | b | c");
        })
    }

    #[test]
    fn pipeline_from_str_unfillable_here_doc_content() {
        block_on(async {
            let result: Result<Pipeline, _> = "<<FOO".parse();
            let e = result.unwrap_err().unwrap();
            assert_eq!(
                e.cause,
                ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
            );
        })
    }

    #[test]
    fn and_or_from_str() {
        assert_eq!(AndOr::from_str("&&"), Ok(AndOr::AndThen));
        assert_eq!(AndOr::from_str("||"), Ok(AndOr::OrElse));
    }

    #[test]
    fn and_or_list_from_str() {
        block_on(async {
            let parse: AndOrList<MissingHereDoc> = " a|b&&! c||d|e ".parse().unwrap();
            let parse = parse.fill(&mut empty()).unwrap();
            assert_eq!(parse.to_string(), "a | b && ! c || d | e");
        })
    }

    #[test]
    fn and_or_list_from_str_unfillable_here_doc_content() {
        block_on(async {
            let result: Result<AndOrList, _> = "<<FOO".parse();
            let e = result.unwrap_err().unwrap();
            assert_eq!(
                e.cause,
                ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
            );
        })
    }

    #[test]
    fn list_from_str() {
        block_on(async {
            let parse: List<MissingHereDoc> = " a;b&&c&d ".parse().unwrap();
            let parse = parse.fill(&mut empty()).unwrap();
            assert_eq!(parse.to_string(), "a; b && c& d");

            let parse: List = " a;b&&c&d ".parse().unwrap();
            assert_eq!(parse.to_string(), "a; b && c& d");
        })
    }

    #[test]
    fn list_from_str_unfillable_here_doc_content() {
        block_on(async {
            let result: Result<List, _> = "<<FOO".parse();
            let e = result.unwrap_err();
            assert_eq!(
                e.cause,
                ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
            );
        })
    }
}
