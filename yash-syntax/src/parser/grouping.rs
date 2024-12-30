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

//! Syntax parser for grouping and subshell

use super::core::Parser;
use super::core::Result;
use super::error::Error;
use super::error::SyntaxError;
use super::lex::Keyword::{CloseBrace, OpenBrace};
use super::lex::Operator::{CloseParen, OpenParen};
use super::lex::TokenId::{Operator, Token};
use crate::syntax::CompoundCommand;
use std::rc::Rc;

impl Parser<'_, '_> {
    /// Parses a normal grouping.
    ///
    /// The next token must be a `{`.
    ///
    /// # Panics
    ///
    /// If the first token is not a `{`.
    pub async fn grouping(&mut self) -> Result<CompoundCommand> {
        let open = self.take_token_raw().await?;
        assert_eq!(open.id, Token(Some(OpenBrace)));

        let list = self.maybe_compound_list_boxed().await?;

        let close = self.take_token_raw().await?;
        if close.id != Token(Some(CloseBrace)) {
            let opening_location = open.word.location;
            let cause = SyntaxError::UnclosedGrouping { opening_location }.into();
            let location = close.word.location;
            return Err(Error { cause, location });
        }

        // TODO allow empty subshell if not POSIXly-correct
        if list.0.is_empty() {
            let cause = SyntaxError::EmptyGrouping.into();
            let location = close.word.location;
            return Err(Error { cause, location });
        }

        Ok(CompoundCommand::Grouping(list))
    }

    /// Parses a subshell.
    ///
    /// The next token must be a `(`.
    ///
    /// # Panics
    ///
    /// If the first token is not a `(`.
    pub async fn subshell(&mut self) -> Result<CompoundCommand> {
        let open = self.take_token_raw().await?;
        assert_eq!(open.id, Operator(OpenParen));

        let list = self.maybe_compound_list_boxed().await?;

        let close = self.take_token_raw().await?;
        if close.id != Operator(CloseParen) {
            let opening_location = open.word.location;
            let cause = SyntaxError::UnclosedSubshell { opening_location }.into();
            let location = close.word.location;
            return Err(Error { cause, location });
        }

        // TODO allow empty subshell if not POSIXly-correct
        if list.0.is_empty() {
            let cause = SyntaxError::EmptySubshell.into();
            let location = close.word.location;
            return Err(Error { cause, location });
        }

        Ok(CompoundCommand::Subshell {
            body: Rc::new(list),
            location: open.word.location,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::lex::Lexer;
    use super::*;
    use crate::alias::{AliasSet, HashEntry};
    use crate::source::Location;
    use crate::source::Source;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn parser_grouping_short() {
        let mut lexer = Lexer::with_code("{ :; }");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.compound_command().now_or_never().unwrap();
        let compound_command = result.unwrap().unwrap();
        assert_matches!(compound_command, CompoundCommand::Grouping(list) => {
            assert_eq!(list.to_string(), ":");
        });
    }

    #[test]
    fn parser_grouping_long() {
        let mut lexer = Lexer::with_code("{ foo; bar& }");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.compound_command().now_or_never().unwrap();
        let compound_command = result.unwrap().unwrap();
        assert_matches!(compound_command, CompoundCommand::Grouping(list) => {
            assert_eq!(list.to_string(), "foo; bar&");
        });
    }

    #[test]
    fn parser_grouping_unclosed() {
        let mut lexer = Lexer::with_code(" { oh no ");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.compound_command().now_or_never().unwrap();
        let e = result.unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedGrouping { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), " { oh no ");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(*opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 1..2);
        });
        assert_eq!(*e.location.code.value.borrow(), " { oh no ");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 9..9);
    }

    #[test]
    fn parser_grouping_empty_posix() {
        let mut lexer = Lexer::with_code("{ }");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.compound_command().now_or_never().unwrap();
        let e = result.unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyGrouping));
        assert_eq!(*e.location.code.value.borrow(), "{ }");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 2..3);
    }

    #[test]
    fn parser_grouping_aliasing() {
        let mut lexer = Lexer::with_code(" { :; end ");
        #[allow(clippy::mutable_key_type)]
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "{".to_string(),
            "".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "}".to_string(),
            "".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "end".to_string(),
            "}".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::config().aliases(&aliases).input(&mut lexer);

        let result = parser.compound_command().now_or_never().unwrap();
        let compound_command = result.unwrap().unwrap();
        assert_matches!(compound_command, CompoundCommand::Grouping(list) => {
            assert_eq!(list.to_string(), ":");
        });
    }

    #[test]
    fn parser_subshell_short() {
        let mut lexer = Lexer::with_code("(:)");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.compound_command().now_or_never().unwrap();
        let compound_command = result.unwrap().unwrap();
        assert_matches!(compound_command, CompoundCommand::Subshell { body, location } => {
            assert_eq!(body.to_string(), ":");
            assert_eq!(*location.code.value.borrow(), "(:)");
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(*location.code.source, Source::Unknown);
            assert_eq!(location.range, 0..1);
        });
    }

    #[test]
    fn parser_subshell_long() {
        let mut lexer = Lexer::with_code("( foo& bar; )");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.compound_command().now_or_never().unwrap();
        let compound_command = result.unwrap().unwrap();
        assert_matches!(compound_command, CompoundCommand::Subshell { body, location } => {
            assert_eq!(body.to_string(), "foo& bar");
            assert_eq!(*location.code.value.borrow(), "( foo& bar; )");
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(*location.code.source, Source::Unknown);
            assert_eq!(location.range, 0..1);
        });
    }

    #[test]
    fn parser_subshell_unclosed() {
        let mut lexer = Lexer::with_code(" ( oh no");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.compound_command().now_or_never().unwrap();
        let e = result.unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedSubshell { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), " ( oh no");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(*opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 1..2);
        });
        assert_eq!(*e.location.code.value.borrow(), " ( oh no");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 8..8);
    }

    #[test]
    fn parser_subshell_empty_posix() {
        let mut lexer = Lexer::with_code("( )");
        let mut parser = Parser::new(&mut lexer);

        let result = parser.compound_command().now_or_never().unwrap();
        let e = result.unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptySubshell));
        assert_eq!(*e.location.code.value.borrow(), "( )");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 2..3);
    }
}
