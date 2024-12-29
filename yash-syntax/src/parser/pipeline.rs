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

use super::core::Parser;
use super::core::Rec;
use super::core::Result;
use super::error::Error;
use super::error::SyntaxError;
use super::lex::Keyword::Bang;
use super::lex::Operator::Bar;
use super::lex::TokenId::{Operator, Token};
use crate::syntax::Pipeline;
use std::rc::Rc;

impl Parser<'_, '_> {
    /// Parses a pipeline.
    ///
    /// If there is no valid pipeline at the current position, this function
    /// returns `Ok(Rec::Parsed(None))`.
    pub async fn pipeline(&mut self) -> Result<Rec<Option<Pipeline>>> {
        // Parse the first command
        let (first, negation) = match self.command().await? {
            Rec::AliasSubstituted => return Ok(Rec::AliasSubstituted),
            Rec::Parsed(Some(first)) => (first, false),
            Rec::Parsed(None) => {
                // Parse the `!` reserved word
                if self.peek_token().await?.id != Token(Some(Bang)) {
                    return Ok(Rec::Parsed(None));
                }
                self.take_token_raw().await?;
                // TODO Warn if `!` is immediately followed by `(`, which is
                // not POSIXly portable.

                // Parse the command after the `!`
                loop {
                    match self.command().await? {
                        Rec::AliasSubstituted => continue,
                        Rec::Parsed(Some(first)) => break (first, true),
                        Rec::Parsed(None) => {
                            // Error: the command is missing
                            let next = self.take_token_raw().await?;
                            let cause = if next.id == Token(Some(Bang)) {
                                SyntaxError::DoubleNegation.into()
                            } else {
                                SyntaxError::MissingCommandAfterBang.into()
                            };
                            let location = next.word.location;
                            return Err(Error { cause, location });
                        }
                    }
                }
            }
        };

        // Parse `|`
        let mut commands = vec![Rc::new(first)];
        while self.peek_token().await?.id == Operator(Bar) {
            self.take_token_raw().await?;

            // Parse the next command
            let next = loop {
                while self.newline_and_here_doc_contents().await? {}

                match self.command().await? {
                    Rec::AliasSubstituted => continue,
                    Rec::Parsed(Some(next)) => break next,
                    Rec::Parsed(None) => {
                        // Error: the command is missing
                        let next = self.take_token_raw().await?;
                        let cause = if next.id == Token(Some(Bang)) {
                            SyntaxError::BangAfterBar.into()
                        } else {
                            SyntaxError::MissingCommandAfterBar.into()
                        };
                        let location = next.word.location;
                        return Err(Error { cause, location });
                    }
                }
            };
            commands.push(Rc::new(next));
        }

        Ok(Rec::Parsed(Some(Pipeline { commands, negation })))
    }
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::lex::Lexer;
    use super::*;
    use crate::alias::{AliasSet, HashEntry};
    use crate::source::Location;
    use crate::source::Source;
    use futures_util::FutureExt;

    #[test]
    fn parser_pipeline_eof() {
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let option = parser.pipeline().now_or_never().unwrap().unwrap().unwrap();
        assert_eq!(option, None);
    }

    #[test]
    fn parser_pipeline_one() {
        let mut lexer = Lexer::from_memory("foo", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.pipeline().now_or_never().unwrap();
        let p = result.unwrap().unwrap().unwrap();
        assert_eq!(p.negation, false);
        assert_eq!(p.commands.len(), 1);
        assert_eq!(p.commands[0].to_string(), "foo");
    }

    #[test]
    fn parser_pipeline_many() {
        let mut lexer = Lexer::from_memory("one | two | \n\t\n three", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.pipeline().now_or_never().unwrap();
        let p = result.unwrap().unwrap().unwrap();
        assert_eq!(p.negation, false);
        assert_eq!(p.commands.len(), 3);
        assert_eq!(p.commands[0].to_string(), "one");
        assert_eq!(p.commands[1].to_string(), "two");
        assert_eq!(p.commands[2].to_string(), "three");
    }

    #[test]
    fn parser_pipeline_negated() {
        let mut lexer = Lexer::from_memory("! foo", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.pipeline().now_or_never().unwrap();
        let p = result.unwrap().unwrap().unwrap();
        assert_eq!(p.negation, true);
        assert_eq!(p.commands.len(), 1);
        assert_eq!(p.commands[0].to_string(), "foo");
    }

    #[test]
    fn parser_pipeline_double_negation() {
        let mut lexer = Lexer::from_memory(" !  !", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let e = parser.pipeline().now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::DoubleNegation));
        assert_eq!(*e.location.code.value.borrow(), " !  !");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 4..5);
    }

    #[test]
    fn parser_pipeline_missing_command_after_negation() {
        let mut lexer = Lexer::from_memory("!\nfoo", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let e = parser.pipeline().now_or_never().unwrap().unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingCommandAfterBang)
        );
        assert_eq!(*e.location.code.value.borrow(), "!\n");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 1..2);
    }

    #[test]
    fn parser_pipeline_missing_command_after_bar() {
        let mut lexer = Lexer::from_memory("foo | ;", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let e = parser.pipeline().now_or_never().unwrap().unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingCommandAfterBar)
        );
        assert_eq!(*e.location.code.value.borrow(), "foo | ;");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 6..7);
    }

    #[test]
    fn parser_pipeline_bang_after_bar() {
        let mut lexer = Lexer::from_memory("foo | !", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let e = parser.pipeline().now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::BangAfterBar));
        assert_eq!(*e.location.code.value.borrow(), "foo | !");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 6..7);
    }

    #[test]
    fn parser_pipeline_no_aliasing_of_bang() {
        let mut lexer = Lexer::from_memory("! ok", Source::Unknown);
        #[allow(clippy::mutable_key_type)]
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "!".to_string(),
            "; ; ;".to_string(),
            true,
            origin,
        ));
        let mut parser = Parser::config().aliases(&aliases).input(&mut lexer);

        let result = parser.pipeline().now_or_never().unwrap();
        let p = result.unwrap().unwrap().unwrap();
        assert_eq!(p.negation, true);
        assert_eq!(p.commands.len(), 1);
        assert_eq!(p.commands[0].to_string(), "ok");
    }

    #[test]
    fn parser_alias_substitution_to_newline_after_bar() {
        let mut lexer = Lexer::from_memory("foo | X\n bar", Source::Unknown);
        #[allow(clippy::mutable_key_type)]
        let mut aliases = AliasSet::new();
        aliases.insert(HashEntry::new(
            "X".to_string(),
            "\n".to_string(),
            false,
            Location::dummy(""),
        ));
        let mut parser = Parser::config().aliases(&aliases).input(&mut lexer);

        let result = parser.pipeline().now_or_never().unwrap();
        let p = result.unwrap().unwrap().unwrap();
        assert_eq!(p.negation, false);
        assert_eq!(p.commands.len(), 2);
        assert_eq!(p.commands[0].to_string(), "foo");
        assert_eq!(p.commands[1].to_string(), "bar");
    }
}
