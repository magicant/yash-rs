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

//! Syntax parser for and-or list

use super::core::Parser;
use super::core::Rec;
use super::core::Result;
use super::error::Error;
use super::error::SyntaxError;
use super::fill::MissingHereDoc;
use super::lex::Operator::{AndAnd, BarBar};
use super::lex::TokenId::Operator;
use crate::syntax::AndOr;
use crate::syntax::AndOrList;

impl Parser<'_, '_> {
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
}

#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::fill::Fill;
    use super::super::lex::Lexer;
    use super::*;
    use crate::source::Source;
    use futures_executor::block_on;

    #[test]
    fn parser_and_or_list_eof() {
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let option = block_on(parser.and_or_list()).unwrap().unwrap();
        assert_eq!(option, None);
    }

    #[test]
    fn parser_and_or_list_one() {
        let mut lexer = Lexer::from_memory("foo", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let aol = block_on(parser.and_or_list()).unwrap().unwrap().unwrap();
        let aol = aol.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(aol.first.to_string(), "foo");
        assert_eq!(aol.rest, vec![]);
    }

    #[test]
    fn parser_and_or_list_many() {
        let mut lexer = Lexer::from_memory("first && second || \n\n third;", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

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
        let mut lexer = Lexer::from_memory("foo &&", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.and_or_list()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingPipeline(AndOr::AndThen))
        );
        assert_eq!(*e.location.code.value.borrow(), "foo &&");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 7);
    }
}
