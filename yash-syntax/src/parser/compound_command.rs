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

//! Syntax parser for compound command
//!
//! Note that the detail parser for each type of compound commands is in another
//! dedicated module.

use super::core::Parser;
use super::core::Result;
use super::error::Error;
use super::error::SyntaxError;
use super::lex::Keyword::{Case, Do, Done, For, If, OpenBrace, Until, While};
use super::lex::Operator::OpenParen;
use super::lex::TokenId::{Operator, Token};
use crate::syntax::CompoundCommand;
use crate::syntax::FullCompoundCommand;
use crate::syntax::List;

impl Parser<'_, '_> {
    /// Parses a `do` clause, i.e., a compound list surrounded in `do ... done`.
    ///
    /// Returns `Ok(None)` if the first token is not `do`.
    pub async fn do_clause(&mut self) -> Result<Option<List>> {
        if self.peek_token().await?.id != Token(Some(Do)) {
            return Ok(None);
        }

        let open = self.take_token_raw().await?;

        let list = self.maybe_compound_list_boxed().await?;

        let close = self.take_token_raw().await?;
        if close.id != Token(Some(Done)) {
            let opening_location = open.word.location;
            let cause = SyntaxError::UnclosedDoClause { opening_location }.into();
            let location = close.word.location;
            return Err(Error { cause, location });
        }

        // TODO allow empty do clause if not POSIXly-correct
        if list.0.is_empty() {
            let cause = SyntaxError::EmptyDoClause.into();
            let location = close.word.location;
            return Err(Error { cause, location });
        }

        Ok(Some(list))
    }

    /// Parses a compound command.
    pub async fn compound_command(&mut self) -> Result<Option<CompoundCommand>> {
        match self.peek_token().await?.id {
            Token(Some(OpenBrace)) => self.grouping().await.map(Some),
            Operator(OpenParen) => self.subshell().await.map(Some),
            Token(Some(For)) => self.for_loop().await.map(Some),
            Token(Some(While)) => self.while_loop().await.map(Some),
            Token(Some(Until)) => self.until_loop().await.map(Some),
            Token(Some(If)) => self.if_command().await.map(Some),
            Token(Some(Case)) => self.case_command().await.map(Some),
            _ => Ok(None),
        }
    }

    /// Parses a compound command with optional redirections.
    pub async fn full_compound_command(&mut self) -> Result<Option<FullCompoundCommand>> {
        let command = match self.compound_command().await? {
            Some(command) => command,
            None => return Ok(None),
        };
        let redirs = self.redirections().await?;

        // POSIX recognizes a reserved word only when it is the first word of a
        // command or follows another reserved word. This compound command ends
        // with a reserved word (`}`, `done`, `fi`, `esac`) unless it is a
        // subshell (which ends with the `)` operator) or has a redirection
        // (which ends with a word). In those cases, a clause-delimiting reserved
        // word that immediately follows (such as the `}` in `{ ( : ) }`) is not
        // portably recognized, so reject it in portable mode.
        let ends_with_reserved_word =
            redirs.is_empty() && !matches!(command, CompoundCommand::Subshell { .. });
        if self.mode().portable && !ends_with_reserved_word {
            let next = self.peek_token().await?;
            if let Token(Some(keyword)) = next.id
                && keyword.is_clause_delimiter()
            {
                let location = next.word.location.clone();
                return Err(Error {
                    cause: SyntaxError::MissingSeparatorBeforeReservedWord.into(),
                    location,
                });
            }
        }

        Ok(Some(FullCompoundCommand { command, redirs }))
    }
}

#[allow(
    clippy::bool_assert_comparison,
    reason = "to make the expected values clearer"
)]
#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::lex::Keyword::CloseBrace;
    use super::super::lex::Lexer;
    use super::super::lex::Operator::Semicolon;
    use super::super::lex::TokenId::EndOfInput;
    use super::*;
    use crate::alias::{AliasSet, HashEntry};
    use crate::source::Location;
    use crate::source::Source;
    use crate::syntax::Command;
    use crate::syntax::ExpansionMode;
    use crate::syntax::SimpleCommand;
    use assert_matches::assert_matches;
    use futures_util::FutureExt as _;

    #[test]
    fn parser_do_clause_none() {
        let mut lexer = Lexer::with_code("done");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.do_clause().now_or_never().unwrap().unwrap();
        assert!(result.is_none(), "result should be none: {result:?}");
    }

    #[test]
    fn parser_do_clause_short() {
        let mut lexer = Lexer::with_code("do :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.do_clause().now_or_never().unwrap().unwrap().unwrap();
        assert_eq!(result.to_string(), ":");

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_do_clause_long() {
        let mut lexer = Lexer::with_code("do foo; bar& done");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.do_clause().now_or_never().unwrap().unwrap().unwrap();
        assert_eq!(result.to_string(), "foo; bar&");

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_do_clause_unclosed() {
        let mut lexer = Lexer::with_code(" do not close ");
        let mut parser = Parser::new(&mut lexer);

        let e = parser.do_clause().now_or_never().unwrap().unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedDoClause { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), " do not close ");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(*opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 1..3);
        });
        assert_eq!(*e.location.code.value.borrow(), " do not close ");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 14..14);
    }

    #[test]
    fn parser_do_clause_empty_posix() {
        let mut lexer = Lexer::with_code("do done");
        let mut parser = Parser::new(&mut lexer);

        let e = parser.do_clause().now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyDoClause));
        assert_eq!(*e.location.code.value.borrow(), "do done");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 3..7);
    }

    #[test]
    fn parser_do_clause_aliasing() {
        let mut lexer = Lexer::with_code(" do :; end ");
        #[allow(clippy::mutable_key_type, reason = "AliasSet is defined as such")]
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "do".to_string(),
            "".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "done".to_string(),
            "".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "end".to_string(),
            "done".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::config().aliases(&aliases).input(&mut lexer);

        let result = parser.do_clause().now_or_never().unwrap().unwrap().unwrap();
        assert_eq!(result.to_string(), ":");

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_compound_command_none() {
        let mut lexer = Lexer::with_code("}");
        let mut parser = Parser::new(&mut lexer);

        let option = parser.compound_command().now_or_never().unwrap().unwrap();
        assert_eq!(option, None);
    }

    #[test]
    fn parser_full_compound_command_without_redirections() {
        let mut lexer = Lexer::with_code("(:)");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.full_compound_command().now_or_never().unwrap();
        let FullCompoundCommand { command, redirs } = result.unwrap().unwrap();
        assert_eq!(command.to_string(), "(:)");
        assert_eq!(redirs, []);
    }

    #[test]
    fn parser_full_compound_command_with_redirections() {
        let mut lexer = Lexer::with_code("(command) <foo >bar ;");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.full_compound_command().now_or_never().unwrap();
        let FullCompoundCommand { command, redirs } = result.unwrap().unwrap();
        assert_eq!(command.to_string(), "(command)");
        assert_eq!(redirs.len(), 2);
        assert_eq!(redirs[0].to_string(), "<foo");
        assert_eq!(redirs[1].to_string(), ">bar");

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, Operator(Semicolon));
    }

    fn portable_mode() -> yash_env::parser::Mode {
        let mut mode = yash_env::parser::Mode::default();
        mode.portable = true;
        mode
    }

    #[test]
    fn parser_full_compound_command_close_brace_after_subshell_rejected_in_portable_mode() {
        // The `}` follows `)`, which is not a reserved word.
        let mut lexer = Lexer::with_code("( : ) }");
        lexer.set_mode(portable_mode());
        let mut parser = Parser::new(&mut lexer);

        let e = parser
            .full_compound_command()
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingSeparatorBeforeReservedWord)
        );
        assert_eq!(*e.location.code.value.borrow(), "( : ) }");
        assert_eq!(e.location.range, 6..7);
    }

    #[test]
    fn parser_full_compound_command_done_after_subshell_rejected_in_portable_mode() {
        // The rule applies to any clause-delimiting reserved word, not just `}`.
        let mut lexer = Lexer::with_code("( : ) done");
        lexer.set_mode(portable_mode());
        let mut parser = Parser::new(&mut lexer);

        let e = parser
            .full_compound_command()
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingSeparatorBeforeReservedWord)
        );
        assert_eq!(e.location.range, 6..10);
    }

    #[test]
    fn parser_full_compound_command_then_after_subshell_rejected_in_portable_mode() {
        let mut lexer = Lexer::with_code("( : ) then");
        lexer.set_mode(portable_mode());
        let mut parser = Parser::new(&mut lexer);

        let e = parser
            .full_compound_command()
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingSeparatorBeforeReservedWord)
        );
    }

    #[test]
    fn parser_full_compound_command_close_brace_after_redirection_rejected_in_portable_mode() {
        let mut lexer = Lexer::with_code("{ :; } >foo }");
        lexer.set_mode(portable_mode());
        let mut parser = Parser::new(&mut lexer);

        let e = parser
            .full_compound_command()
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingSeparatorBeforeReservedWord)
        );
    }

    #[test]
    fn parser_full_compound_command_close_brace_allowed_without_portable() {
        let mut lexer = Lexer::with_code("( : ) }");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.full_compound_command().now_or_never().unwrap();
        let FullCompoundCommand { command, .. } = result.unwrap().unwrap();
        assert_eq!(command.to_string(), "(:)");

        // The `}` is left unconsumed.
        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, Token(Some(CloseBrace)));
    }

    #[test]
    fn parser_full_compound_command_separator_before_close_brace_allowed_in_portable_mode() {
        let mut lexer = Lexer::with_code("( : ) ;");
        lexer.set_mode(portable_mode());
        let mut parser = Parser::new(&mut lexer);

        let result = parser.full_compound_command().now_or_never().unwrap();
        let FullCompoundCommand { command, .. } = result.unwrap().unwrap();
        assert_eq!(command.to_string(), "(:)");
    }

    #[test]
    fn parser_full_compound_command_close_brace_after_grouping_allowed_in_portable_mode() {
        // The `}` follows the inner grouping's `}`, which is a reserved word, so
        // it is portable and must be accepted.
        let mut lexer = Lexer::with_code("{ :; } }");
        lexer.set_mode(portable_mode());
        let mut parser = Parser::new(&mut lexer);

        let result = parser.full_compound_command().now_or_never().unwrap();
        let FullCompoundCommand { command, redirs } = result.unwrap().unwrap();
        assert_matches!(command, CompoundCommand::Grouping(_));
        assert_eq!(redirs, []);

        // The outer `}` is left unconsumed for the enclosing grouping.
        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, Token(Some(CloseBrace)));
    }

    #[test]
    fn parser_full_compound_command_close_brace_after_if_allowed_in_portable_mode() {
        // The `}` follows `fi`, which is a reserved word.
        let mut lexer = Lexer::with_code("if true; then :; fi }");
        lexer.set_mode(portable_mode());
        let mut parser = Parser::new(&mut lexer);

        let result = parser.full_compound_command().now_or_never().unwrap();
        let FullCompoundCommand { command, redirs } = result.unwrap().unwrap();
        assert_matches!(command, CompoundCommand::If { .. });
        assert_eq!(redirs, []);

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, Token(Some(CloseBrace)));
    }

    #[test]
    fn parser_full_compound_command_none() {
        let mut lexer = Lexer::with_code("}");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.full_compound_command().now_or_never().unwrap();
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn parser_short_function_definition_ok() {
        let mut lexer = Lexer::with_code(" ( ) ( : ) > /dev/null ");
        let mut parser = Parser::new(&mut lexer);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec![("foo".parse().unwrap(), ExpansionMode::Multiple)],
            redirs: vec![].into(),
        };

        let result = parser.short_function_definition(c).now_or_never().unwrap();
        let command = result.unwrap();
        assert_matches!(command, Command::Function(f) => {
            assert_eq!(f.has_keyword, false);
            assert_eq!(f.name.to_string(), "foo");
            assert_eq!(f.body.to_string(), "(:) >/dev/null");
        });

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }
}
