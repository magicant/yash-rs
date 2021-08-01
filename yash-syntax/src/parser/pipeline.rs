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

//! Syntax parser for pipeline

use super::core::Error;
use super::core::Parser;
use super::core::Rec;
use super::core::Result;
use super::core::SyntaxError;
use super::fill::MissingHereDoc;
use super::lex::Keyword::Bang;
use super::lex::Operator::Bar;
use super::lex::TokenId::{Operator, Token};
use crate::syntax::Pipeline;
use std::rc::Rc;

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
                    // TODO Warn if `!` is immediately followed by `(`, which is
                    // not POSIXly portable.
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
        let mut commands = vec![Rc::new(first)];
        while self.peek_token().await?.id == Operator(Bar) {
            let bar_location = self.take_token_raw().await?.word.location;

            while self.newline_and_here_doc_contents().await? {}

            commands.push(loop {
                // Parse the next command
                if let Rec::Parsed(option) = self.command().await? {
                    if let Some(next) = option {
                        break Rc::new(next);
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
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::super::core::ErrorCause;
    use super::super::fill::Fill;
    use super::super::lex::Lexer;
    use super::*;
    use crate::alias::{AliasSet, HashEntry};
    use crate::source::Location;
    use crate::source::Source;
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
        let origin = Location::dummy("");
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
}
