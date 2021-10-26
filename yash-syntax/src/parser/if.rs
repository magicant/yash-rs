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

//! Syntax parser for if command

use super::core::Parser;
use super::core::Result;
use super::error::Error;
use super::error::SyntaxError;
use super::fill::MissingHereDoc;
use super::lex::Keyword::{Elif, Else, Fi, If, Then};
use super::lex::TokenId::Token;
use crate::syntax::CompoundCommand;
use crate::syntax::ElifThen;

impl Parser<'_, '_> {
    /// Parses an elif-then clause.
    ///
    /// Returns `Ok(None)` if the next token is not `elif`.
    async fn elif_then_clause(&mut self) -> Result<Option<ElifThen<MissingHereDoc>>> {
        if self.peek_token().await?.id != Token(Some(Elif)) {
            return Ok(None);
        }

        let elif = self.take_token_raw().await?;

        let condition = self.maybe_compound_list_boxed().await?;
        let then = self.take_token_raw().await?;

        // TODO allow empty condition if not POSIXly-correct
        if condition.0.is_empty() {
            let cause = SyntaxError::EmptyElifCondition.into();
            let location = then.word.location;
            return Err(Error { cause, location });
        }
        if then.id != Token(Some(Then)) {
            let elif_location = elif.word.location;
            let cause = SyntaxError::ElifMissingThen { elif_location }.into();
            let location = then.word.location;
            return Err(Error { cause, location });
        }

        let body = self.maybe_compound_list_boxed().await?;
        // TODO allow empty body if not POSIXly-correct
        if body.0.is_empty() {
            let cause = SyntaxError::EmptyElifBody.into();
            let location = self.take_token_raw().await?.word.location;
            return Err(Error { cause, location });
        }

        Ok(Some(ElifThen { condition, body }))
    }

    /// Parses an if conditional construct.
    ///
    /// The next token must be the `if` reserved word.
    ///
    /// # Panics
    ///
    /// If the first token is not `if`.
    pub async fn if_command(&mut self) -> Result<CompoundCommand<MissingHereDoc>> {
        let open = self.take_token_raw().await?;
        assert_eq!(open.id, Token(Some(If)));

        let condition = self.maybe_compound_list_boxed().await?;
        let then = self.take_token_raw().await?;

        // TODO allow empty condition if not POSIXly-correct
        if condition.0.is_empty() {
            let cause = SyntaxError::EmptyIfCondition.into();
            let location = then.word.location;
            return Err(Error { cause, location });
        }
        if then.id != Token(Some(Then)) {
            let if_location = open.word.location;
            let cause = SyntaxError::IfMissingThen { if_location }.into();
            let location = then.word.location;
            return Err(Error { cause, location });
        }

        let body = self.maybe_compound_list_boxed().await?;
        // TODO allow empty body if not POSIXly-correct
        if body.0.is_empty() {
            let cause = SyntaxError::EmptyIfBody.into();
            let location = self.take_token_raw().await?.word.location;
            return Err(Error { cause, location });
        }

        let mut elifs = Vec::new();
        while let Some(elif) = self.elif_then_clause().await? {
            elifs.push(elif);
        }

        let r#else = if self.peek_token().await?.id == Token(Some(Else)) {
            self.take_token_raw().await?;
            let content = self.maybe_compound_list_boxed().await?;
            // TODO allow empty else if not POSIXly-correct
            if content.0.is_empty() {
                let cause = SyntaxError::EmptyElse.into();
                let location = self.take_token_raw().await?.word.location;
                return Err(Error { cause, location });
            }
            Some(content)
        } else {
            None
        };

        let fi = self.take_token_raw().await?;
        if fi.id != Token(Some(Fi)) {
            let opening_location = open.word.location;
            let cause = SyntaxError::UnclosedIf { opening_location }.into();
            let location = fi.word.location;
            return Err(Error { cause, location });
        }

        Ok(CompoundCommand::If {
            condition,
            body,
            elifs,
            r#else,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::lex::Lexer;
    use super::super::lex::TokenId::EndOfInput;
    use super::*;
    use crate::source::Source;
    use futures_executor::block_on;

    #[test]
    fn parser_if_command_minimum() {
        let mut lexer = Lexer::from_memory("if a; then b; fi", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        if let CompoundCommand::If {
            condition,
            body,
            elifs,
            r#else,
        } = result
        {
            assert_eq!(condition.to_string(), "a");
            assert_eq!(body.to_string(), "b");
            assert_eq!(elifs, []);
            assert_eq!(r#else, None);
        } else {
            panic!("Not an if command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_if_command_one_elif() {
        let mut lexer = Lexer::from_memory(
            "if\ntrue\nthen\nfalse\n\nelif x; then y& fi",
            Source::Unknown,
        );
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        if let CompoundCommand::If {
            condition,
            body,
            elifs,
            r#else,
        } = result
        {
            assert_eq!(condition.to_string(), "true");
            assert_eq!(body.to_string(), "false");
            assert_eq!(elifs.len(), 1);
            assert_eq!(elifs[0].to_string(), "elif x; then y&");
            assert_eq!(r#else, None);
        } else {
            panic!("Not an if command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_if_command_many_elifs() {
        let mut lexer = Lexer::from_memory(
            "if a; then b; elif c; then d; elif e 1; e 2& then f 1; f 2& elif g; then h; fi",
            Source::Unknown,
        );
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        if let CompoundCommand::If {
            condition,
            body,
            elifs,
            r#else,
        } = result
        {
            assert_eq!(condition.to_string(), "a");
            assert_eq!(body.to_string(), "b");
            assert_eq!(elifs.len(), 3);
            assert_eq!(elifs[0].to_string(), "elif c; then d");
            assert_eq!(elifs[1].to_string(), "elif e 1; e 2& then f 1; f 2&");
            assert_eq!(elifs[2].to_string(), "elif g; then h");
            assert_eq!(r#else, None);
        } else {
            panic!("Not an if command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_if_command_else() {
        let mut lexer = Lexer::from_memory("if a; then b; else c; d; fi", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        if let CompoundCommand::If {
            condition,
            body,
            elifs,
            r#else,
        } = result
        {
            assert_eq!(condition.to_string(), "a");
            assert_eq!(body.to_string(), "b");
            assert_eq!(elifs, []);
            assert_eq!(r#else.unwrap().to_string(), "c; d");
        } else {
            panic!("Not an if command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_if_command_elif_and_else() {
        let mut lexer =
            Lexer::from_memory("if 1; then 2; elif 3; then 4; else 5; fi", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        if let CompoundCommand::If {
            condition,
            body,
            elifs,
            r#else,
        } = result
        {
            assert_eq!(condition.to_string(), "1");
            assert_eq!(body.to_string(), "2");
            assert_eq!(elifs.len(), 1);
            assert_eq!(elifs[0].to_string(), "elif 3; then 4");
            assert_eq!(r#else.unwrap().to_string(), "5");
        } else {
            panic!("Not an if command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_if_command_without_then_after_if() {
        let mut lexer = Lexer::from_memory(" if :; fi", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::IfMissingThen { if_location }) = e.cause {
            assert_eq!(if_location.line.value, " if :; fi");
            assert_eq!(if_location.line.number.get(), 1);
            assert_eq!(if_location.line.source, Source::Unknown);
            assert_eq!(if_location.column.get(), 2);
        } else {
            panic!("Wrong error cause: {:?}", e);
        }
        assert_eq!(e.location.line.value, " if :; fi");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 8);
    }

    #[test]
    fn parser_if_command_without_then_after_elif() {
        let mut lexer = Lexer::from_memory("if a; then b; elif c; fi", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::ElifMissingThen { elif_location }) = e.cause {
            assert_eq!(elif_location.line.value, "if a; then b; elif c; fi");
            assert_eq!(elif_location.line.number.get(), 1);
            assert_eq!(elif_location.line.source, Source::Unknown);
            assert_eq!(elif_location.column.get(), 15);
        } else {
            panic!("Wrong error cause: {:?}", e);
        }
        assert_eq!(e.location.line.value, "if a; then b; elif c; fi");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 23);
    }

    #[test]
    fn parser_if_command_without_fi() {
        let mut lexer = Lexer::from_memory("  if :; then :; }", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedIf { opening_location }) = e.cause {
            assert_eq!(opening_location.line.value, "  if :; then :; }");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 3);
        } else {
            panic!("Wrong error cause: {:?}", e);
        }
        assert_eq!(e.location.line.value, "  if :; then :; }");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 17);
    }

    #[test]
    fn parser_if_command_empty_condition() {
        let mut lexer = Lexer::from_memory("   if then :; fi", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyIfCondition));
        assert_eq!(e.location.line.value, "   if then :; fi");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 7);
    }

    #[test]
    fn parser_if_command_empty_body() {
        let mut lexer = Lexer::from_memory("if :; then fi", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyIfBody));
        assert_eq!(e.location.line.value, "if :; then fi");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 12);
    }

    #[test]
    fn parser_if_command_empty_elif_condition() {
        let mut lexer = Lexer::from_memory("if :; then :; elif then :; fi", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyElifCondition));
        assert_eq!(e.location.line.value, "if :; then :; elif then :; fi");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 20);
    }

    #[test]
    fn parser_if_command_empty_elif_body() {
        let mut lexer = Lexer::from_memory("if :; then :; elif :; then fi", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyElifBody));
        assert_eq!(e.location.line.value, "if :; then :; elif :; then fi");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 28);
    }

    #[test]
    fn parser_if_command_empty_else() {
        let mut lexer = Lexer::from_memory("if :; then :; else fi", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyElse));
        assert_eq!(e.location.line.value, "if :; then :; else fi");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 20);
    }
}
