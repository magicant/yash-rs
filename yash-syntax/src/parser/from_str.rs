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
use std::future::Future;
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

/// Parses a [`TextUnit`] by `lexer.text_unit(|_| false, |_| true)`.
impl FromStr for TextUnit {
    type Err = Error;
    fn from_str(s: &str) -> Result<TextUnit, Error> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        unwrap_ready(lexer.text_unit(|_| false, |_| true)).map(Option::unwrap)
    }
}

// Parses a text by `lexer.text(|_| false, |_| true)`.
impl FromStr for Text {
    type Err = Error;
    fn from_str(s: &str) -> Result<Text, Error> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        unwrap_ready(lexer.text(|_| false, |_| true))
    }
}

impl FromStr for WordUnit {
    type Err = Error;
    fn from_str(s: &str) -> Result<WordUnit, Error> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        unwrap_ready(lexer.word_unit(|_| false)).map(Option::unwrap)
    }
}

/// Converts a string to a word.
///
/// This implementation does not parse any tilde expansions in the word.
/// To parse them, you need to call [`Word::parse_tilde_front`] or
/// [`Word::parse_tilde_everywhere`] on the resultant word.
impl FromStr for Word {
    type Err = Error;

    fn from_str(s: &str) -> Result<Word, Error> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
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

/// Converts a string to an assignment.
impl FromStr for Assign {
    /// Optional error value
    ///
    /// The error is `None` if the input is a valid word but not an assignment.
    /// A proper error is returned in `Some(_)` in case of a syntax error.
    type Err = Option<Error>;

    fn from_str(s: &str) -> Result<Assign, Option<Error>> {
        let c: SimpleCommand = s.parse()?;
        Ok(c.assigns.into_iter().next()).shift()
    }
}

impl FromStr for Operator {
    type Err = ();
    fn from_str(s: &str) -> Result<Operator, ()> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
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

/// Converts a string to a redirection.
///
/// This implementation does not support parsing a here-document.
impl FromStr for Redir {
    /// Optional error value
    ///
    /// The error is `None` if the first token is not a redirection operator.
    /// A proper error is returned in `Some(_)` in case of a syntax error.
    type Err = Option<Error>;

    fn from_str(s: &str) -> Result<Redir, Option<Error>> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let redir = unwrap_ready(parser.redirection()).shift()?;
        parser.ensure_no_unread_here_doc()?;
        // If this redirection is a here-document, its content cannot be filled
        // because there is no newline token that would make the content to be
        // read.
        Ok(redir)
    }
}

/// Converts a string to a simple command.
///
/// This implementation does not support parsing a command that contains a
/// here-document.
impl FromStr for SimpleCommand {
    /// Optional error value
    ///
    /// The error is `None` if the first token does not start a simple command.
    /// A proper error is returned in `Some(_)` in case of a syntax error.
    type Err = Option<Error>;

    fn from_str(s: &str) -> Result<SimpleCommand, Option<Error>> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let command = unwrap_ready(parser.simple_command())
            .map(Rec::unwrap)
            .shift()?;
        parser.ensure_no_unread_here_doc()?;
        // If the simple command contains a here-document, its content cannot be
        // filled because there is no newline token that would make the content
        // to be read.
        Ok(command)
    }
}

/// Converts a string to a case item.
impl FromStr for CaseItem {
    /// Optional error value
    ///
    /// The error is `None` if the first token is `esac`.
    /// A proper error is returned in `Some(_)` in case of a syntax error.
    type Err = Option<Error>;

    fn from_str(s: &str) -> Result<CaseItem, Option<Error>> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let item = unwrap_ready(parser.case_item()).shift()?;
        parser.ensure_no_unread_here_doc()?;
        Ok(item)
    }
}

/// Converts a string to a compound command.
impl FromStr for CompoundCommand {
    /// Optional error value
    ///
    /// The error is `None` if the first token does not start a compound command.
    /// A proper error is returned in `Some(_)` in case of a syntax error.
    type Err = Option<Error>;

    fn from_str(s: &str) -> Result<CompoundCommand, Option<Error>> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let command = unwrap_ready(parser.compound_command()).shift()?;
        parser.ensure_no_unread_here_doc()?;
        Ok(command)
    }
}

/// Converts a string to a compound command.
impl FromStr for FullCompoundCommand {
    /// Optional error value
    ///
    /// The error is `None` if the first token does not start a compound command.
    /// A proper error is returned in `Some(_)` in case of a syntax error.
    type Err = Option<Error>;

    fn from_str(s: &str) -> Result<FullCompoundCommand, Option<Error>> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let command = unwrap_ready(parser.full_compound_command()).shift()?;
        parser.ensure_no_unread_here_doc()?;
        Ok(command)
    }
}

/// Converts a string to a command.
impl FromStr for Command {
    /// Optional error value
    ///
    /// The error is `None` if the first token does not start a command.
    /// A proper error is returned in `Some(_)` in case of a syntax error.
    type Err = Option<Error>;

    fn from_str(s: &str) -> Result<Command, Option<Error>> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let command = unwrap_ready(parser.command()).map(Rec::unwrap).shift()?;
        parser.ensure_no_unread_here_doc()?;
        Ok(command)
    }
}

/// Converts a string to a pipeline.
impl FromStr for Pipeline {
    /// Optional error value
    ///
    /// The error is `None` if the first token does not start a pipeline.
    /// A proper error is returned in `Some(_)` in case of a syntax error.
    type Err = Option<Error>;

    fn from_str(s: &str) -> Result<Pipeline, Option<Error>> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let pipeline = unwrap_ready(parser.pipeline()).map(Rec::unwrap).shift()?;
        parser.ensure_no_unread_here_doc()?;
        Ok(pipeline)
    }
}

impl FromStr for AndOr {
    type Err = ();
    fn from_str(s: &str) -> Result<AndOr, ()> {
        Operator::from_str(s)?.try_into()
    }
}

/// Converts a string to an and-or list.
impl FromStr for AndOrList {
    /// Optional error value
    ///
    /// The error is `None` if the first token does not start an and-or list.
    /// A proper error is returned in `Some(_)` in case of a syntax error.
    type Err = Option<Error>;

    fn from_str(s: &str) -> Result<AndOrList, Option<Error>> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let list = unwrap_ready(parser.and_or_list())
            .map(Rec::unwrap)
            .shift()?;
        parser.ensure_no_unread_here_doc()?;
        Ok(list)
    }
}

/// Converts a string to a list.
impl FromStr for List {
    type Err = Error;
    fn from_str(s: &str) -> Result<List, Error> {
        let mut lexer = Lexer::from_memory(s, Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let list = unwrap_ready(parser.maybe_compound_list())?;
        parser.ensure_no_unread_here_doc()?;
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ErrorCause;
    use crate::parser::SyntaxError;
    use assert_matches::assert_matches;
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
            assert_matches!(&parse.0[2], CommandSubst { content, .. } => {
                assert_eq!(&**content, "c");
            });
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
            let parse: Redir = "2> /dev/null".parse().unwrap();
            assert_eq!(parse.fd, Some(Fd(2)));
            assert_matches!(parse.body, RedirBody::Normal { operator, operand } => {
                assert_eq!(operator, RedirOp::FileOut);
                assert_eq!(operand.to_string(), "/dev/null");
            });
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
            let parse: SimpleCommand = " a=b</dev/null foo ".parse().unwrap();
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
            let parse: CaseItem = " foo) ".parse().unwrap();
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
            let parse: CompoundCommand = " { :; } ".parse().unwrap();
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
            let parse: FullCompoundCommand = " { :; } <&- ".parse().unwrap();
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
            let parse: Command = "f(){ :; }>&2".parse().unwrap();
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
            let parse: Pipeline = " ! a|b|c".parse().unwrap();
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
            let parse: AndOrList = " a|b&&! c||d|e ".parse().unwrap();
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
