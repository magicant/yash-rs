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

//! Syntax parser for function definition command

use super::core::Parser;
use super::core::Rec;
use super::core::Result;
use super::error::Error;
use super::error::SyntaxError;
use super::lex::Operator::{CloseParen, OpenParen};
use super::lex::TokenId::{Operator, Token};
use crate::syntax::Command;
use crate::syntax::FunctionDefinition;
use crate::syntax::SimpleCommand;
use std::rc::Rc;

impl Parser<'_, '_> {
    /// Parses a function definition command that does not start with the
    /// `function` reserved word.
    ///
    /// This function must be called just after a [simple
    /// command](Self::simple_command) has been parsed.
    /// The simple command must be passed as an argument.
    /// If the simple command has only one word and the next token is `(`, it is
    /// parsed as a function definition command.
    /// Otherwise, the simple command is returned intact.
    pub async fn short_function_definition(&mut self, mut intro: SimpleCommand) -> Result<Command> {
        if !intro.is_one_word() || self.peek_token().await?.id != Operator(OpenParen) {
            return Ok(Command::Simple(intro));
        }

        let open = self.take_token_raw().await?;
        debug_assert_eq!(open.id, Operator(OpenParen));

        let close = self.take_token_auto(&[]).await?;
        if close.id != Operator(CloseParen) {
            return Err(Error {
                cause: SyntaxError::UnmatchedParenthesis.into(),
                location: close.word.location,
            });
        }

        let name = intro.words.pop().unwrap();
        debug_assert!(intro.is_empty());
        // TODO reject invalid name if POSIXly-correct

        loop {
            while self.newline_and_here_doc_contents().await? {}

            return match self.full_compound_command().await? {
                Some(body) => Ok(Command::Function(FunctionDefinition {
                    has_keyword: false,
                    name,
                    body: Rc::new(body),
                })),
                None => {
                    let next = match self.take_token_manual(false).await? {
                        Rec::AliasSubstituted => continue,
                        Rec::Parsed(next) => next,
                    };
                    let cause = if let Token(_) = next.id {
                        SyntaxError::InvalidFunctionBody.into()
                    } else {
                        SyntaxError::MissingFunctionBody.into()
                    };
                    let location = next.word.location;
                    Err(Error { cause, location })
                }
            };
        }
    }
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::lex::Lexer;
    use super::super::lex::TokenId::EndOfInput;
    use super::*;
    use crate::alias::{AliasSet, HashEntry};
    use crate::source::Location;
    use crate::source::Source;
    use assert_matches::assert_matches;
    use futures_executor::block_on;

    #[test]
    fn parser_short_function_definition_not_one_word_name() {
        let mut lexer = Lexer::from_memory("(", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec![],
            redirs: vec![].into(),
        };

        let result = block_on(parser.short_function_definition(c)).unwrap();
        assert_matches!(result, Command::Simple(c) => {
            assert_eq!(c.to_string(), "");
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, Operator(OpenParen));
    }

    #[test]
    fn parser_short_function_definition_eof() {
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["foo".parse().unwrap()],
            redirs: vec![].into(),
        };

        let result = block_on(parser.short_function_definition(c)).unwrap();
        assert_matches!(result, Command::Simple(c) => {
            assert_eq!(c.to_string(), "foo");
        });
    }

    #[test]
    fn parser_short_function_definition_unmatched_parenthesis() {
        let mut lexer = Lexer::from_memory("( ", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["foo".parse().unwrap()],
            redirs: vec![].into(),
        };

        let e = block_on(parser.short_function_definition(c)).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::UnmatchedParenthesis)
        );
        assert_eq!(*e.location.code.value.borrow(), "( ");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 2..2);
    }

    #[test]
    fn parser_short_function_definition_missing_function_body() {
        let mut lexer = Lexer::from_memory("( ) ", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["foo".parse().unwrap()],
            redirs: vec![].into(),
        };

        let e = block_on(parser.short_function_definition(c)).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingFunctionBody)
        );
        assert_eq!(*e.location.code.value.borrow(), "( ) ");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 4..4);
    }

    #[test]
    fn parser_short_function_definition_invalid_function_body() {
        let mut lexer = Lexer::from_memory("() foo ; ", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["foo".parse().unwrap()],
            redirs: vec![].into(),
        };

        let e = block_on(parser.short_function_definition(c)).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::InvalidFunctionBody)
        );
        assert_eq!(*e.location.code.value.borrow(), "() foo ; ");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 3..6);
    }

    #[test]
    fn parser_short_function_definition_close_parenthesis_alias() {
        let mut lexer = Lexer::from_memory(" a b ", Source::Unknown);
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "a".to_string(),
            "f( ".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "b".to_string(),
            " c".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "c".to_string(),
            " )\n\n(:)".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(async {
            parser.simple_command().await.unwrap(); // alias
            let c = parser.simple_command().await.unwrap().unwrap().unwrap();
            parser.short_function_definition(c).await.unwrap()
        });
        assert_matches!(result, Command::Function(f) => {
            assert_eq!(f.has_keyword, false);
            assert_eq!(f.name.to_string(), "f");
            assert_eq!(f.body.to_string(), "(:)");
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_short_function_definition_body_alias_and_newline() {
        let mut lexer = Lexer::from_memory(" a b ", Source::Unknown);
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "a".to_string(),
            "f() ".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "b".to_string(),
            " c".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "c".to_string(),
            "\n\n(:)".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(async {
            parser.simple_command().await.unwrap(); // alias
            let c = parser.simple_command().await.unwrap().unwrap().unwrap();
            parser.short_function_definition(c).await.unwrap()
        });
        assert_matches!(result, Command::Function(f) => {
            assert_eq!(f.has_keyword, false);
            assert_eq!(f.name.to_string(), "f");
            assert_eq!(f.body.to_string(), "(:)");
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_short_function_definition_alias_inapplicable() {
        let mut lexer = Lexer::from_memory("()b", Source::Unknown);
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "b".to_string(),
            " c".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "c".to_string(),
            "(:)".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::new(&mut lexer, &aliases);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["f".parse().unwrap()],
            redirs: vec![].into(),
        };

        let e = block_on(parser.short_function_definition(c)).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::InvalidFunctionBody)
        );
        assert_eq!(*e.location.code.value.borrow(), "()b");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 2..3);
    }
}
