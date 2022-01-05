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
use super::fill::MissingHereDoc;
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
    pub async fn do_clause(&mut self) -> Result<Option<List<MissingHereDoc>>> {
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
    pub async fn compound_command(&mut self) -> Result<Option<CompoundCommand<MissingHereDoc>>> {
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
    pub async fn full_compound_command(
        &mut self,
    ) -> Result<Option<FullCompoundCommand<MissingHereDoc>>> {
        let command = match self.compound_command().await? {
            Some(command) => command,
            None => return Ok(None),
        };
        let redirs = self.redirections().await?;
        // TODO Reject `{ { :; } >foo }` and `{ ( : ) }` if POSIXly-correct
        // (The last `}` is not regarded as a keyword in these cases.)
        Ok(Some(FullCompoundCommand { command, redirs }))
    }
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::fill::Fill;
    use super::super::lex::Lexer;
    use super::super::lex::Operator::Semicolon;
    use super::super::lex::TokenId::EndOfInput;
    use super::*;
    use crate::alias::{AliasSet, HashEntry};
    use crate::source::Location;
    use crate::source::Source;
    use crate::syntax::Command;
    use crate::syntax::SimpleCommand;
    use futures_executor::block_on;

    #[test]
    fn parser_do_clause_none() {
        let mut lexer = Lexer::from_memory("done", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.do_clause()).unwrap();
        assert!(result.is_none(), "result should be none: {:?}", result);
    }

    #[test]
    fn parser_do_clause_short() {
        let mut lexer = Lexer::from_memory("do :; done", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.do_clause()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(result.to_string(), ":");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_do_clause_long() {
        let mut lexer = Lexer::from_memory("do foo; bar& done", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.do_clause()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(result.to_string(), "foo; bar&");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_do_clause_unclosed() {
        let mut lexer = Lexer::from_memory(" do not close ", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.do_clause()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedDoClause { opening_location }) = e.cause {
            assert_eq!(opening_location.code.value, " do not close ");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 2);
        } else {
            panic!("Wrong error cause: {:?}", e.cause);
        }
        assert_eq!(e.location.code.value, " do not close ");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 15);
    }

    #[test]
    fn parser_do_clause_empty_posix() {
        let mut lexer = Lexer::from_memory("do done", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.do_clause()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyDoClause));
        assert_eq!(e.location.code.value, "do done");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 4);
    }

    #[test]
    fn parser_do_clause_aliasing() {
        let mut lexer = Lexer::from_memory(" do :; end ", Source::Unknown);
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
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.do_clause()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(result.to_string(), ":");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_compound_command_none() {
        let mut lexer = Lexer::from_memory("}", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let option = block_on(parser.compound_command()).unwrap();
        assert_eq!(option, None);
    }

    #[test]
    fn parser_full_compound_command_without_redirections() {
        let mut lexer = Lexer::from_memory("(:)", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.full_compound_command()).unwrap().unwrap();
        let FullCompoundCommand { command, redirs } = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(command.to_string(), "(:)");
        assert_eq!(redirs, []);
    }

    #[test]
    fn parser_full_compound_command_with_redirections() {
        let mut lexer = Lexer::from_memory("(command) <foo >bar ;", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.full_compound_command()).unwrap().unwrap();
        let FullCompoundCommand { command, redirs } = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(command.to_string(), "(command)");
        assert_eq!(redirs.len(), 2);
        assert_eq!(redirs[0].to_string(), "<foo");
        assert_eq!(redirs[1].to_string(), ">bar");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, Operator(Semicolon));
    }

    #[test]
    fn parser_full_compound_command_none() {
        let mut lexer = Lexer::from_memory("}", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let option = block_on(parser.full_compound_command()).unwrap();
        assert_eq!(option, None);
    }

    #[test]
    fn parser_short_function_definition_ok() {
        let mut lexer = Lexer::from_memory(" ( ) ( : ) > /dev/null ", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["foo".parse().unwrap()],
            redirs: vec![].into(),
        };

        let result = block_on(parser.short_function_definition(c)).unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let Command::Function(f) = result {
            assert_eq!(f.has_keyword, false);
            assert_eq!(f.name.to_string(), "foo");
            assert_eq!(f.body.to_string(), "(:) >/dev/null");
        } else {
            panic!("Not a function definition: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }
}
