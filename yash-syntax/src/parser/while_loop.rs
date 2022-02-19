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

use super::core::Parser;
use super::core::Result;
use super::error::Error;
use super::error::SyntaxError;
use super::lex::Keyword::{Until, While};
use super::lex::TokenId::Token;
use crate::source::Location;
use crate::syntax::CompoundCommand;

impl Parser<'_, '_> {
    /// Parses a while loop.
    ///
    /// The next token must be the `while` reserved word.
    ///
    /// # Panics
    ///
    /// If the first token is not `while`.
    pub async fn while_loop(&mut self) -> Result<CompoundCommand> {
        let open = self.take_token_raw().await?;
        assert_eq!(open.id, Token(Some(While)));

        let condition = self.maybe_compound_list_boxed().await?;

        // TODO allow empty condition if not POSIXly-correct
        if condition.0.is_empty() {
            let cause = SyntaxError::EmptyWhileCondition.into();
            let next = self.take_token_raw().await?;
            let location = Location {
                code: next.word.span.code,
                index: next.word.span.range.start,
            };
            return Err(Error { cause, location });
        }

        let body = match self.do_clause().await? {
            Some(body) => body,
            None => {
                let opening_location = Location {
                    code: open.word.span.code,
                    index: open.word.span.range.start,
                };
                let cause = SyntaxError::UnclosedWhileClause { opening_location }.into();
                let next = self.take_token_raw().await?;
                let location = Location {
                    code: next.word.span.code,
                    index: next.word.span.range.start,
                };
                return Err(Error { cause, location });
            }
        };

        Ok(CompoundCommand::While { condition, body })
    }

    /// Parses an until loop.
    ///
    /// The next token must be the `until` reserved word.
    ///
    /// # Panics
    ///
    /// If the first token is not `until`.
    pub async fn until_loop(&mut self) -> Result<CompoundCommand> {
        let open = self.take_token_raw().await?;
        assert_eq!(open.id, Token(Some(Until)));

        let condition = self.maybe_compound_list_boxed().await?;

        // TODO allow empty condition if not POSIXly-correct
        if condition.0.is_empty() {
            let cause = SyntaxError::EmptyUntilCondition.into();
            let next = self.take_token_raw().await?;
            let location = Location {
                code: next.word.span.code,
                index: next.word.span.range.start,
            };
            return Err(Error { cause, location });
        }

        let body = match self.do_clause().await? {
            Some(body) => body,
            None => {
                let opening_location = Location {
                    code: open.word.span.code,
                    index: open.word.span.range.start,
                };
                let cause = SyntaxError::UnclosedUntilClause { opening_location }.into();
                let next = self.take_token_raw().await?;
                let location = Location {
                    code: next.word.span.code,
                    index: next.word.span.range.start,
                };
                return Err(Error { cause, location });
            }
        };

        Ok(CompoundCommand::Until { condition, body })
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::lex::Lexer;
    use super::super::lex::TokenId::EndOfInput;
    use super::*;
    use crate::alias::{AliasSet, HashEntry};
    use crate::source::Source;
    use crate::source::Span;
    use assert_matches::assert_matches;
    use futures_executor::block_on;

    #[test]
    fn parser_while_loop_short() {
        let mut lexer = Lexer::from_memory("while true; do :; done", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_matches!(result, CompoundCommand::While { condition, body } => {
            assert_eq!(condition.to_string(), "true");
            assert_eq!(body.to_string(), ":");
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_while_loop_long() {
        let mut lexer = Lexer::from_memory("while false; true& do foo; bar& done", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_matches!(result, CompoundCommand::While { condition, body } => {
            assert_eq!(condition.to_string(), "false; true&");
            assert_eq!(body.to_string(), "foo; bar&");
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_while_loop_unclosed() {
        let mut lexer = Lexer::from_memory("while :", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedWhileClause { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "while :");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.index, 0);
        });
        assert_eq!(*e.location.code.value.borrow(), "while :");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 7);
    }

    #[test]
    fn parser_while_loop_empty_posix() {
        let mut lexer = Lexer::from_memory(" while do :; done", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::EmptyWhileCondition)
        );
        assert_eq!(*e.location.code.value.borrow(), " while do :; done");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 7);
    }

    #[test]
    fn parser_while_loop_aliasing() {
        let mut lexer = Lexer::from_memory(" while :; DO :; done", Source::Unknown);
        let mut aliases = AliasSet::new();
        let origin = Span::dummy("");
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
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_eq!(result.to_string(), "while :; do :; done");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_until_loop_short() {
        let mut lexer = Lexer::from_memory("until true; do :; done", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_matches!(result, CompoundCommand::Until { condition, body } => {
            assert_eq!(condition.to_string(), "true");
            assert_eq!(body.to_string(), ":");
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_until_loop_long() {
        let mut lexer = Lexer::from_memory("until false; true& do foo; bar& done", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_matches!(result, CompoundCommand::Until { condition, body } => {
            assert_eq!(condition.to_string(), "false; true&");
            assert_eq!(body.to_string(), "foo; bar&");
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_until_loop_unclosed() {
        let mut lexer = Lexer::from_memory("until :", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedUntilClause { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "until :");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.index, 0);
        });
        assert_eq!(*e.location.code.value.borrow(), "until :");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 7);
    }

    #[test]
    fn parser_until_loop_empty_posix() {
        let mut lexer = Lexer::from_memory("  until do :; done", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::EmptyUntilCondition)
        );
        assert_eq!(*e.location.code.value.borrow(), "  until do :; done");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 8);
    }

    #[test]
    fn parser_until_loop_aliasing() {
        let mut lexer = Lexer::from_memory(" until :; DO :; done", Source::Unknown);
        let mut aliases = AliasSet::new();
        let origin = Span::dummy("");
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
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_eq!(result.to_string(), "until :; do :; done");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }
}
