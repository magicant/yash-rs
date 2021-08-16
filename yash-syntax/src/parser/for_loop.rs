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

//! Syntax parser for for loop

use super::core::Error;
use super::core::Parser;
use super::core::Rec;
use super::core::Result;
use super::core::SyntaxError;
use super::fill::MissingHereDoc;
use super::lex::Keyword::{Do, For, In};
use super::lex::Operator::{Newline, Semicolon};
use super::lex::TokenId::{EndOfInput, IoNumber, Operator, Token};
use crate::source::Location;
use crate::syntax::CompoundCommand;
use crate::syntax::List;
use crate::syntax::Word;

impl Parser<'_> {
    /// Parses the name of a for loop.
    async fn for_loop_name(&mut self) -> Result<Word> {
        let name = self.take_token_auto(&[]).await?;

        // Validate the token type
        match name.id {
            EndOfInput | Operator(Newline) | Operator(Semicolon) => {
                let cause = SyntaxError::MissingForName.into();
                let location = name.word.location;
                return Err(Error { cause, location });
            }
            Operator(_) => {
                let cause = SyntaxError::InvalidForName.into();
                let location = name.word.location;
                return Err(Error { cause, location });
            }
            Token(_) | IoNumber => (),
        }

        // TODO reject non-portable names in POSIXly-correct mode

        Ok(name.word)
    }

    /// Parses the values of a for loop.
    ///
    /// For the values to be parsed, the first token needs to be `in`. Otherwise,
    /// the result will be `None`.
    ///
    /// If successful, `opening_location` is returned intact as the second value
    /// of the tuple.
    async fn for_loop_values(
        &mut self,
        opening_location: Location,
    ) -> Result<(Option<Vec<Word>>, Location)> {
        // Parse the `in`
        let mut first_line = true;
        loop {
            match self.peek_token().await?.id {
                Operator(Semicolon) if first_line => {
                    self.take_token_raw().await?;
                    return Ok((None, opening_location));
                }
                Token(Some(Do)) => {
                    return Ok((None, opening_location));
                }
                Operator(Newline) => {
                    assert!(self.newline_and_here_doc_contents().await?);
                    first_line = false;
                }
                Token(Some(In)) => {
                    self.take_token_raw().await?;
                    break;
                }
                _ => match self.take_token_manual(false).await? {
                    Rec::AliasSubstituted => (),
                    Rec::Parsed(token) => {
                        let cause = SyntaxError::MissingForBody { opening_location }.into();
                        let location = token.word.location;
                        return Err(Error { cause, location });
                    }
                },
            }
        }

        // Parse values until a delimiter is found
        let mut values = Vec::new();
        loop {
            let next = self.take_token_auto(&[]).await?;
            match next.id {
                Token(_) | IoNumber => {
                    values.push(next.word);
                }
                Operator(Semicolon) | Operator(Newline) => {
                    return Ok((Some(values), opening_location));
                }
                _ => {
                    let cause = SyntaxError::InvalidForValue.into();
                    let location = next.word.location;
                    return Err(Error { cause, location });
                }
            }
        }
    }

    /// Parses the body of a for loop, possibly preceded by newlines.
    async fn for_loop_body(&mut self, opening_location: Location) -> Result<List<MissingHereDoc>> {
        loop {
            while self.newline_and_here_doc_contents().await? {}

            if let Some(body) = self.do_clause().await? {
                return Ok(body);
            }

            match self.take_token_manual(false).await? {
                Rec::AliasSubstituted => (),
                Rec::Parsed(token) => {
                    let cause = SyntaxError::MissingForBody { opening_location }.into();
                    let location = token.word.location;
                    return Err(Error { cause, location });
                }
            }
        }
    }

    /// Parses a for loop.
    ///
    /// The next token must be the `for` reserved word.
    ///
    /// # Panics
    ///
    /// If the first token is not `for`.
    pub async fn for_loop(&mut self) -> Result<CompoundCommand<MissingHereDoc>> {
        let open = self.take_token_raw().await?;
        assert_eq!(open.id, Token(Some(For)));
        let opening_location = open.word.location;

        let name = self.for_loop_name().await?;
        let (values, opening_location) = self.for_loop_values(opening_location).await?;
        let body = self.for_loop_body(opening_location).await?;
        Ok(CompoundCommand::For { name, values, body })
    }
}

#[cfg(test)]
mod tests {
    use super::super::core::ErrorCause;
    use super::super::fill::Fill;
    use super::super::lex::Lexer;
    use super::*;
    use crate::alias::{AliasSet, HashEntry};
    use crate::source::Source;
    use futures_executor::block_on;

    #[test]
    fn parser_for_loop_short() {
        let mut lexer = Lexer::with_source(Source::Unknown, "for A do :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::For { name, values, body } = result {
            assert_eq!(name.to_string(), "A");
            assert_eq!(values, None);
            assert_eq!(body.to_string(), ":")
        } else {
            panic!("Not a for loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_for_loop_with_semicolon_before_do() {
        let mut lexer = Lexer::with_source(Source::Unknown, "for B ; do :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::For { name, values, body } = result {
            assert_eq!(name.to_string(), "B");
            assert_eq!(values, None);
            assert_eq!(body.to_string(), ":")
        } else {
            panic!("Not a for loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_for_loop_with_semicolon_and_newlines_before_do() {
        let mut lexer = Lexer::with_source(Source::Unknown, "for B ; \n\t\n do :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::For { name, values, body } = result {
            assert_eq!(name.to_string(), "B");
            assert_eq!(values, None);
            assert_eq!(body.to_string(), ":")
        } else {
            panic!("Not a for loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_for_loop_with_newlines_before_do() {
        let mut lexer = Lexer::with_source(Source::Unknown, "for B \n \\\n \n do :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::For { name, values, body } = result {
            assert_eq!(name.to_string(), "B");
            assert_eq!(values, None);
            assert_eq!(body.to_string(), ":")
        } else {
            panic!("Not a for loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_for_loop_with_zero_values_delimited_by_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, "for foo in; do :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::For { name, values, body } = result {
            assert_eq!(name.to_string(), "foo");
            assert_eq!(values, Some(vec![]));
            assert_eq!(body.to_string(), ":")
        } else {
            panic!("Not a for loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_for_loop_with_one_value_delimited_by_semicolon_and_newlines() {
        let mut lexer = Lexer::with_source(Source::Unknown, "for foo in bar; \n \n do :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::For { name, values, body } = result {
            assert_eq!(name.to_string(), "foo");
            let values = values
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>();
            assert_eq!(values, vec!["bar"]);
            assert_eq!(body.to_string(), ":")
        } else {
            panic!("Not a for loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_for_loop_with_many_values_delimited_by_one_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, "for in in in a b c\ndo :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::For { name, values, body } = result {
            assert_eq!(name.to_string(), "in");
            let values = values
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>();
            assert_eq!(values, vec!["in", "a", "b", "c"]);
            assert_eq!(body.to_string(), ":")
        } else {
            panic!("Not a for loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_for_loop_with_zero_values_delimited_by_many_newlines() {
        let mut lexer = Lexer::with_source(Source::Unknown, "for foo in \n \n \n do :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::For { name, values, body } = result {
            assert_eq!(name.to_string(), "foo");
            assert_eq!(values, Some(vec![]));
            assert_eq!(body.to_string(), ":")
        } else {
            panic!("Not a for loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_for_loop_newlines_before_in() {
        let mut lexer = Lexer::with_source(Source::Unknown, "for foo\n \n\nin\ndo :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::For { name, values, body } = result {
            assert_eq!(name.to_string(), "foo");
            assert_eq!(values, Some(vec![]));
            assert_eq!(body.to_string(), ":")
        } else {
            panic!("Not a for loop: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_for_loop_aliasing_on_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, " FOR_A if :; done");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "if".to_string(),
            " ;\n\ndo".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "FOR_A".to_string(),
            "for A ".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let first_pass = block_on(parser.take_token_manual(true)).unwrap();
        assert!(first_pass.is_alias_substituted());

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(result.to_string(), "for A do :; done");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_for_loop_aliasing_on_do() {
        let mut lexer = Lexer::with_source(Source::Unknown, " FOR_A if :; done");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "if".to_string(),
            "\ndo".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "FOR_A".to_string(),
            "for A ".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let first_pass = block_on(parser.take_token_manual(true)).unwrap();
        assert!(first_pass.is_alias_substituted());

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(result.to_string(), "for A do :; done");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_for_loop_missing_name_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, " for ");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MissingForName));
        assert_eq!(e.location.line.value, " for ");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 6);
    }

    #[test]
    fn parser_for_loop_missing_name_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, " for\ndo :; done");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MissingForName));
        assert_eq!(e.location.line.value, " for\n");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 5);
    }

    #[test]
    fn parser_for_loop_missing_name_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, "for; do :; done");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MissingForName));
        assert_eq!(e.location.line.value, "for; do :; done");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 4);
    }

    #[test]
    fn parser_for_loop_invalid_name() {
        // Alias substitution results in "for & do :; done"
        let mut lexer = Lexer::with_source(Source::Unknown, "FOR if do :; done");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "FOR".to_string(),
            "for ".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "if".to_string(),
            "&".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let first_pass = block_on(parser.take_token_manual(true)).unwrap();
        assert!(first_pass.is_alias_substituted());

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidForName));
        assert_eq!(e.location.line.value, "&");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.column.get(), 1);
        if let Source::Alias { original, alias } = &e.location.line.source {
            assert_eq!(original.line.value, "FOR if do :; done");
            assert_eq!(original.line.number.get(), 1);
            assert_eq!(original.line.source, Source::Unknown);
            assert_eq!(original.column.get(), 5);
            assert_eq!(alias.name, "if");
        } else {
            panic!("Not an alias: {:?}", e.location.line.source);
        }
    }

    #[test]
    fn parser_for_loop_semicolon_after_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, "for X\n; do :; done");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::MissingForBody { opening_location }) = &e.cause {
            assert_eq!(opening_location.line.value, "for X\n");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("Not MissingForBody: {:?}", e.cause);
        }
        assert_eq!(e.location.line.value, "; do :; done");
        assert_eq!(e.location.line.number.get(), 2);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }

    #[test]
    fn parser_for_loop_invalid_values_delimiter() {
        // Alias substitution results in "for A in a b & c; do :; done"
        let mut lexer = Lexer::with_source(Source::Unknown, "for_A_in_a_b if c; do :; done");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
        aliases.insert(HashEntry::new(
            "for_A_in_a_b".to_string(),
            "for A in a b ".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "if".to_string(),
            "&".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let first_pass = block_on(parser.take_token_manual(true)).unwrap();
        assert!(first_pass.is_alias_substituted());

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidForValue));
        assert_eq!(e.location.line.value, "&");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.column.get(), 1);
        if let Source::Alias { original, alias } = &e.location.line.source {
            assert_eq!(original.line.value, "for_A_in_a_b if c; do :; done");
            assert_eq!(original.line.number.get(), 1);
            assert_eq!(original.line.source, Source::Unknown);
            assert_eq!(original.column.get(), 14);
            assert_eq!(alias.name, "if");
        } else {
            panic!("Not an alias: {:?}", e.location.line.source);
        }
    }

    #[test]
    fn parser_for_loop_invalid_token_after_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, " for X; ! do :; done");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::MissingForBody { opening_location }) = &e.cause {
            assert_eq!(opening_location.line.value, " for X; ! do :; done");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 2);
        } else {
            panic!("Not MissingForBody: {:?}", e.cause);
        }
        assert_eq!(e.location.line.value, " for X; ! do :; done");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 9);
    }
}
