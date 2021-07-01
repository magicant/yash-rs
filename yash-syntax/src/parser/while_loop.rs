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

//! Syntax parser for while and until loops

use super::core::Error;
use super::core::Parser;
use super::core::Result;
use super::core::SyntaxError;
use super::fill::MissingHereDoc;
use super::lex::Keyword::{Until, While};
use super::lex::TokenId::Token;
use crate::syntax::CompoundCommand;

impl Parser<'_> {
    /// Parses a while loop.
    ///
    /// The next token must be the `while` reserved word.
    ///
    /// # Panics
    ///
    /// If the first token is not `while`.
    pub async fn while_loop(&mut self) -> Result<CompoundCommand<MissingHereDoc>> {
        let open = self.take_token_raw().await?;
        assert_eq!(open.id, Token(Some(While)));

        let condition = self.maybe_compound_list_boxed().await?;

        let body = match self.do_clause().await? {
            Some(body) => body,
            None => {
                let opening_location = open.word.location;
                let cause = SyntaxError::UnclosedWhileClause { opening_location }.into();
                let location = self.take_token_raw().await?.word.location;
                return Err(Error { cause, location });
            }
        };

        // TODO allow empty condition if not POSIXly-correct
        if condition.0.is_empty() {
            let cause = SyntaxError::EmptyWhileCondition.into();
            let location = open.word.location;
            return Err(Error { cause, location });
        }

        Ok(CompoundCommand::While { condition, body })
    }

    /// Parses an until loop.
    ///
    /// The next token must be the `until` reserved word.
    ///
    /// # Panics
    ///
    /// If the first token is not `until`.
    pub async fn until_loop(&mut self) -> Result<CompoundCommand<MissingHereDoc>> {
        let open = self.take_token_raw().await?;
        assert_eq!(open.id, Token(Some(Until)));

        let condition = self.maybe_compound_list_boxed().await?;

        let body = match self.do_clause().await? {
            Some(body) => body,
            None => {
                let opening_location = open.word.location;
                let cause = SyntaxError::UnclosedUntilClause { opening_location }.into();
                let location = self.take_token_raw().await?.word.location;
                return Err(Error { cause, location });
            }
        };

        // TODO allow empty condition if not POSIXly-correct
        if condition.0.is_empty() {
            let cause = SyntaxError::EmptyUntilCondition.into();
            let location = open.word.location;
            return Err(Error { cause, location });
        }

        Ok(CompoundCommand::Until { condition, body })
    }
}

#[cfg(test)]
mod tests {
    use super::super::core::ErrorCause;
    use super::super::fill::Fill;
    use super::super::lex::Lexer;
    use super::super::lex::TokenId::EndOfInput;
    use super::*;
    use crate::alias::{AliasSet, HashEntry};
    use crate::source::Location;
    use crate::source::Source;
    use futures::executor::block_on;

    #[test]
    fn parser_while_loop_short() {
        let mut lexer = Lexer::with_source(Source::Unknown, "while true; do :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::While { condition, body } = result {
            assert_eq!(condition.to_string(), "true");
            assert_eq!(body.to_string(), ":");
        } else {
            panic!("Not a while loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_while_loop_long() {
        let mut lexer = Lexer::with_source(Source::Unknown, "while false; true& do foo; bar& done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::While { condition, body } = result {
            assert_eq!(condition.to_string(), "false; true&");
            assert_eq!(body.to_string(), "foo; bar&");
        } else {
            panic!("Not a while loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_while_loop_unclosed() {
        let mut lexer = Lexer::with_source(Source::Unknown, "while :");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedWhileClause { opening_location }) = e.cause {
            assert_eq!(opening_location.line.value, "while :");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("Wrong error cause: {:?}", e.cause);
        }
        assert_eq!(e.location.line.value, "while :");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 8);
    }

    #[test]
    fn parser_while_loop_empty_posix() {
        let mut lexer = Lexer::with_source(Source::Unknown, " while do :; done");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::EmptyWhileCondition)
        );
        assert_eq!(e.location.line.value, " while do :; done");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 2);
    }

    #[test]
    fn parser_while_loop_aliasing() {
        let mut lexer = Lexer::with_source(Source::Unknown, " while :; DO :; done");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "DO".to_string(),
            "do".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "while".to_string(),
            ";;".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(result.to_string(), "while :; do :; done");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_until_loop_short() {
        let mut lexer = Lexer::with_source(Source::Unknown, "until true; do :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::Until { condition, body } = result {
            assert_eq!(condition.to_string(), "true");
            assert_eq!(body.to_string(), ":");
        } else {
            panic!("Not an until loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_until_loop_long() {
        let mut lexer = Lexer::with_source(Source::Unknown, "until false; true& do foo; bar& done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::Until { condition, body } = result {
            assert_eq!(condition.to_string(), "false; true&");
            assert_eq!(body.to_string(), "foo; bar&");
        } else {
            panic!("Not an until loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_until_loop_unclosed() {
        let mut lexer = Lexer::with_source(Source::Unknown, "until :");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedUntilClause { opening_location }) = e.cause {
            assert_eq!(opening_location.line.value, "until :");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("Wrong error cause: {:?}", e.cause);
        }
        assert_eq!(e.location.line.value, "until :");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 8);
    }

    #[test]
    fn parser_until_loop_empty_posix() {
        let mut lexer = Lexer::with_source(Source::Unknown, "  until do :; done");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::EmptyUntilCondition)
        );
        assert_eq!(e.location.line.value, "  until do :; done");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 3);
    }

    #[test]
    fn parser_until_loop_aliasing() {
        let mut lexer = Lexer::with_source(Source::Unknown, " until :; DO :; done");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "DO".to_string(),
            "do".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "until".to_string(),
            ";;".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(result.to_string(), "until :; do :; done");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }
}
