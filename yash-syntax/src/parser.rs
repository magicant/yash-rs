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

//! Syntax parser for the shell language.
//!
//! TODO Elaborate

mod core;
mod fill;
mod fromstr;

mod for_loop;
mod grouping;
mod redir;
mod simple_command;

pub mod lex;

use self::lex::keyword::Keyword::*;
use self::lex::Operator::*;
use self::lex::TokenId::{self, EndOfInput, Operator, Token};
use super::syntax::*;
use std::future::Future;
use std::pin::Pin;

pub use self::core::AsyncFnMut;
pub use self::core::Error;
pub use self::core::Parser;
pub use self::core::Rec;
pub use self::core::Result;
pub use self::core::SyntaxError;
pub use self::fill::Fill;
pub use self::fill::MissingHereDoc;

impl Parser<'_> {
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
            let location = open.word.location;
            return Err(Error { cause, location });
        }

        Ok(Some(list))
    }

    /// Parses a while loop.
    ///
    /// The next token must be the `while` reserved word.
    ///
    /// # Panics
    ///
    /// If the first token is not `while`.
    async fn while_loop(&mut self) -> Result<CompoundCommand<MissingHereDoc>> {
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
    async fn until_loop(&mut self) -> Result<CompoundCommand<MissingHereDoc>> {
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

    /// Parses a case item.
    ///
    /// Does not parse the optional trailing double semicolon.
    ///
    /// Returns `None` if the next token is `esac`.
    pub async fn case_item(&mut self) -> Result<Option<CaseItem<MissingHereDoc>>> {
        fn pattern_error_cause(token_id: TokenId) -> SyntaxError {
            match token_id {
                Token(Some(Esac)) => SyntaxError::EsacAsPattern,
                Token(_) => unreachable!(),
                Operator(CloseParen) | Operator(Bar) | Operator(Newline) | EndOfInput => {
                    SyntaxError::MissingPattern
                }
                _ => SyntaxError::InvalidPattern,
            }
        }

        let first_token = loop {
            while self.newline_and_here_doc_contents().await? {}

            if self.peek_token().await?.id == Token(Some(Esac)) {
                return Ok(None);
            }

            match self.take_token_manual(false).await? {
                Rec::AliasSubstituted => (),
                Rec::Parsed(token) => break token,
            }
        };

        let first_pattern = match first_token.id {
            Token(_) => first_token.word,
            Operator(OpenParen) => {
                let next_token = self.take_token_auto(&[Esac]).await?;
                match next_token.id {
                    // TODO Allow `esac` if not in POSIXly-correct mode
                    Token(keyword) if keyword != Some(Esac) => next_token.word,
                    _ => {
                        let cause = pattern_error_cause(next_token.id).into();
                        let location = next_token.word.location;
                        return Err(Error { cause, location });
                    }
                }
            }
            _ => {
                let cause = pattern_error_cause(first_token.id).into();
                let location = first_token.word.location;
                return Err(Error { cause, location });
            }
        };

        let mut patterns = vec![first_pattern];
        loop {
            let separator = self.take_token_auto(&[]).await?;
            match separator.id {
                Operator(CloseParen) => break,
                Operator(Bar) => {
                    let pattern = self.take_token_auto(&[]).await?;
                    match pattern.id {
                        Token(_) => patterns.push(pattern.word),
                        _ => {
                            let cause = pattern_error_cause(pattern.id).into();
                            let location = pattern.word.location;
                            return Err(Error { cause, location });
                        }
                    }
                }
                _ => {
                    let cause = SyntaxError::UnclosedPatternList.into();
                    let location = separator.word.location;
                    return Err(Error { cause, location });
                }
            }
        }

        let body = self.maybe_compound_list_boxed().await?;

        Ok(Some(CaseItem { patterns, body }))
    }

    /// Parses a case conditional construct.
    ///
    /// The next token must be the `case` reserved word.
    ///
    /// # Panics
    ///
    /// If the first token is not `case`.
    async fn case_command(&mut self) -> Result<CompoundCommand<MissingHereDoc>> {
        let open = self.take_token_raw().await?;
        assert_eq!(open.id, Token(Some(Case)));

        let subject = self.take_token_auto(&[]).await?;
        match subject.id {
            Token(_) => (),
            Operator(Newline) | EndOfInput => {
                let cause = SyntaxError::MissingCaseSubject.into();
                let location = subject.word.location;
                return Err(Error { cause, location });
            }
            _ => {
                let cause = SyntaxError::InvalidCaseSubject.into();
                let location = subject.word.location;
                return Err(Error { cause, location });
            }
        }
        let subject = subject.word;

        loop {
            while self.newline_and_here_doc_contents().await? {}

            let next_token = self.take_token_auto(&[In]).await?;
            match next_token.id {
                Token(Some(In)) => break,
                Operator(Newline) => (),
                _ => {
                    let opening_location = open.word.location;
                    let cause = SyntaxError::MissingIn { opening_location }.into();
                    let location = next_token.word.location;
                    return Err(Error { cause, location });
                }
            }
        }

        let mut items = Vec::new();
        while let Some(item) = self.case_item().await? {
            items.push(item);

            if self.peek_token().await?.id != Operator(SemicolonSemicolon) {
                break;
            }
            self.take_token_raw().await?;
        }

        let close = self.take_token_raw().await?;
        if close.id != Token(Some(Esac)) {
            let opening_location = open.word.location;
            let cause = SyntaxError::UnclosedCase { opening_location }.into();
            let location = close.word.location;
            return Err(Error { cause, location });
        }

        Ok(CompoundCommand::Case { subject, items })
    }

    /// Parses a compound command.
    pub async fn compound_command(&mut self) -> Result<Option<CompoundCommand<MissingHereDoc>>> {
        match self.peek_token().await?.id {
            Token(Some(OpenBrace)) => self.grouping().await.map(Some),
            Operator(OpenParen) => self.subshell().await.map(Some),
            Token(Some(For)) => self.for_loop().await.map(Some),
            Token(Some(While)) => self.while_loop().await.map(Some),
            Token(Some(Until)) => self.until_loop().await.map(Some),
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

    /// Parses a function definition command that does not start with the
    /// `function` reserved word.
    ///
    /// This function must be called just after a [simple
    /// command](Self::simple_command) has been parsed.
    /// The simple command must be passed as an argument.
    /// If the simple command has only one word and the next token is `(`, it is
    /// parsed as a function definition command.
    /// Otherwise, the simple command is returned intact.
    pub async fn short_function_definition(
        &mut self,
        mut intro: SimpleCommand<MissingHereDoc>,
    ) -> Result<Command<MissingHereDoc>> {
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
                    body,
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

    /// Parses a command.
    ///
    /// If there is no valid command at the current position, this function
    /// returns `Ok(Rec::Parsed(None))`.
    pub async fn command(&mut self) -> Result<Rec<Option<Command<MissingHereDoc>>>> {
        match self.simple_command().await? {
            Rec::AliasSubstituted => Ok(Rec::AliasSubstituted),
            Rec::Parsed(None) => self
                .full_compound_command()
                .await
                .map(|c| Rec::Parsed(c.map(Command::Compound))),
            Rec::Parsed(Some(c)) => self
                .short_function_definition(c)
                .await
                .map(|c| Rec::Parsed(Some(c))),
        }
    }

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
        let mut commands = vec![first];
        while self.peek_token().await?.id == Operator(Bar) {
            let bar_location = self.take_token_raw().await?.word.location;

            while self.newline_and_here_doc_contents().await? {}

            commands.push(loop {
                // Parse the next command
                if let Rec::Parsed(option) = self.command().await? {
                    if let Some(next) = option {
                        break next;
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

    // There is no function that parses a single item because it would not be
    // very useful for parsing a list. An item requires a separator operator
    // ('&' or ';') for it to be followed by another item. You cannot tell from
    // the resultant item whether there was a separator operator.
    // pub async fn item(&mut self) -> Result<Rec<Item<MissingHereDoc>>> { }

    /// Parses a list.
    ///
    /// This function parses a sequence of and-or lists that are separated by `;`
    /// or `&`. A newline token that delimits the list is not parsed.
    ///
    /// If there is no valid command at the current position, this function
    /// returns a list with no items.
    pub async fn list(&mut self) -> Result<Rec<List<MissingHereDoc>>> {
        let mut items = vec![];

        let mut result = match self.and_or_list().await? {
            Rec::AliasSubstituted => return Ok(Rec::AliasSubstituted),
            Rec::Parsed(result) => result,
        };

        while let Some(and_or) = result {
            let (is_async, next) = match self.peek_token().await?.id {
                Operator(Semicolon) => (false, true),
                Operator(And) => (true, true),
                _ => (false, false),
            };

            items.push(Item { and_or, is_async });

            if !next {
                break;
            }
            self.take_token_raw().await?;

            result = loop {
                if let Rec::Parsed(result) = self.and_or_list().await? {
                    break result;
                }
            };
        }

        Ok(Rec::Parsed(List(items)))
    }

    /// Parses an optional newline token and here-document contents.
    ///
    /// If the current token is a newline, it is consumed and any pending here-document contents
    /// are read starting from the next line. Otherwise, this function returns `Ok(false)` without
    /// any side effect.
    pub async fn newline_and_here_doc_contents(&mut self) -> Result<bool> {
        if self.peek_token().await?.id != Operator(Newline) {
            return Ok(false);
        }

        self.take_token_raw().await?;
        self.here_doc_contents().await?;
        Ok(true)
    }

    /// Parses a complete command optionally delimited by a newline.
    ///
    /// A complete command is a minimal sequence of and-or lists that can be executed in the shell
    /// environment. This function reads as many lines as needed to compose the complete command.
    ///
    /// If the current line is empty (or containing only whitespaces and comments), the result is
    /// an empty list. If the first token of the current line is the end of input, the result is
    /// `Ok(None)`.
    pub async fn command_line(&mut self) -> Result<Option<List>> {
        let list = loop {
            if let Rec::Parsed(list) = self.list().await? {
                break list;
            }
        };

        if !self.newline_and_here_doc_contents().await? {
            let next = self.peek_token().await?;
            if next.id != EndOfInput {
                // TODO Return a better error depending on the token id of the peeked token
                return Err(Error {
                    cause: SyntaxError::UnexpectedToken.into(),
                    location: next.word.location.clone(),
                });
            }
            if list.0.is_empty() {
                return Ok(None);
            }
        }

        self.ensure_no_unread_here_doc()?;
        let mut here_docs = self.take_read_here_docs().into_iter();
        let list = list.fill(&mut here_docs)?;
        Ok(Some(list))
    }

    /// Parses an optional compound list.
    ///
    /// A compound list is a sequence of one or more and-or lists that are
    /// separated by newlines and optionally preceded and/or followed by
    /// newlines.
    ///
    /// This function stops parsing on encountering an unexpected token that
    /// cannot be parsed as the beginning of an and-or list. The caller should
    /// check that the next token is an expected one.
    pub async fn maybe_compound_list(&mut self) -> Result<List<MissingHereDoc>> {
        let mut items = vec![];

        loop {
            let list = loop {
                if let Rec::Parsed(list) = self.list().await? {
                    break list;
                }
            };
            items.extend(list.0);

            if !self.newline_and_here_doc_contents().await? {
                break;
            }
        }

        Ok(List(items))
    }

    /// Like [`maybe_compound_list`](Self::maybe_compound_list), but returns the future in a pinned box.
    pub fn maybe_compound_list_boxed(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<List<MissingHereDoc>>> + '_>> {
        Box::pin(self.maybe_compound_list())
    }
}

#[cfg(test)]
mod tests {
    use super::core::ErrorCause;
    use super::lex::Lexer;
    use super::*;
    use crate::alias::{AliasSet, HashEntry};
    use crate::source::{Location, Source};
    use futures::executor::block_on;

    #[test]
    fn parser_do_clause_none() {
        let mut lexer = Lexer::with_source(Source::Unknown, "done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.do_clause()).unwrap();
        assert!(result.is_none(), "result should be none: {:?}", result);
    }

    #[test]
    fn parser_do_clause_short() {
        let mut lexer = Lexer::with_source(Source::Unknown, "do :; done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.do_clause()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(result.to_string(), ":");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_do_clause_long() {
        let mut lexer = Lexer::with_source(Source::Unknown, "do foo; bar& done");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.do_clause()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(result.to_string(), "foo; bar&");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_do_clause_unclosed() {
        let mut lexer = Lexer::with_source(Source::Unknown, " do not close ");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.do_clause()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedDoClause { opening_location }) = e.cause {
            assert_eq!(opening_location.line.value, " do not close ");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 2);
        } else {
            panic!("Wrong error cause: {:?}", e.cause);
        }
        assert_eq!(e.location.line.value, " do not close ");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 15);
    }

    #[test]
    fn parser_do_clause_empty_posix() {
        let mut lexer = Lexer::with_source(Source::Unknown, "do done");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.do_clause()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyDoClause));
        assert_eq!(e.location.line.value, "do done");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }

    #[test]
    fn parser_do_clause_aliasing() {
        let mut lexer = Lexer::with_source(Source::Unknown, " do :; end ");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("".to_string());
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
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let result = block_on(parser.do_clause()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(result.to_string(), ":");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

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
        let origin = Location::dummy("".to_string());
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
        let origin = Location::dummy("".to_string());
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

    #[test]
    fn parser_case_item_esac() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\nESAC");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("".to_string());
        aliases.insert(HashEntry::new(
            "ESAC".to_string(),
            "\n\nesac".to_string(),
            true,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "esac".to_string(),
            "&&".to_string(),
            true,
            origin,
        ));
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let option = block_on(parser.case_item()).unwrap();
        assert_eq!(option, None);

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, Token(Some(Esac)));
    }

    #[test]
    fn parser_case_item_minimum() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo)");
        let mut parser = Parser::new(&mut lexer);

        let item = block_on(parser.case_item()).unwrap().unwrap();
        assert_eq!(item.patterns.len(), 1);
        assert_eq!(item.patterns[0].to_string(), "foo");
        assert_eq!(item.body.0, []);

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_item_with_open_paren() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(foo)");
        let mut parser = Parser::new(&mut lexer);

        let item = block_on(parser.case_item()).unwrap().unwrap();
        assert_eq!(item.patterns.len(), 1);
        assert_eq!(item.patterns[0].to_string(), "foo");
        assert_eq!(item.body.0, []);

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_item_many_patterns() {
        let mut lexer = Lexer::with_source(Source::Unknown, "1 | esac | $three)");
        let mut parser = Parser::new(&mut lexer);

        let item = block_on(parser.case_item()).unwrap().unwrap();
        assert_eq!(item.patterns.len(), 3);
        assert_eq!(item.patterns[0].to_string(), "1");
        assert_eq!(item.patterns[1].to_string(), "esac");
        assert_eq!(item.patterns[2].to_string(), "$three");
        assert_eq!(item.body.0, []);

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_item_non_empty_body() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo)\necho ok\n:&\n");
        let mut parser = Parser::new(&mut lexer);

        let item = block_on(parser.case_item()).unwrap().unwrap();
        assert_eq!(item.patterns.len(), 1);
        assert_eq!(item.patterns[0].to_string(), "foo");
        assert_eq!(item.body.0.len(), 2);
        assert_eq!(item.body.0[0].to_string(), "echo ok");
        assert_eq!(item.body.0[1].to_string(), ":&");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_item_with_double_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo);;");
        let mut parser = Parser::new(&mut lexer);

        let item = block_on(parser.case_item()).unwrap().unwrap();
        assert_eq!(item.patterns.len(), 1);
        assert_eq!(item.patterns[0].to_string(), "foo");
        assert_eq!(item.body.0, []);

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, Operator(SemicolonSemicolon));
    }

    #[test]
    fn parser_case_item_with_non_empty_body_and_double_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo):;\n;;");
        let mut parser = Parser::new(&mut lexer);

        let item = block_on(parser.case_item()).unwrap().unwrap();
        assert_eq!(item.patterns.len(), 1);
        assert_eq!(item.patterns[0].to_string(), "foo");
        assert_eq!(item.body.0.len(), 1);
        assert_eq!(item.body.0[0].to_string(), ":");

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, Operator(SemicolonSemicolon));
    }

    #[test]
    fn parser_case_item_missing_pattern_without_open_paren() {
        let mut lexer = Lexer::with_source(Source::Unknown, ")");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.case_item()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MissingPattern));
        assert_eq!(e.location.line.value, ")");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }

    #[test]
    fn parser_case_item_esac_after_paren() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(esac)");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.case_item()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EsacAsPattern));
        assert_eq!(e.location.line.value, "(esac)");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 2);
    }

    #[test]
    fn parser_case_item_first_pattern_not_word_after_open_paren() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(&");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.case_item()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidPattern));
        assert_eq!(e.location.line.value, "(&");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 2);
    }

    #[test]
    fn parser_case_item_missing_pattern_after_bar() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(foo| |");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.case_item()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MissingPattern));
        assert_eq!(e.location.line.value, "(foo| |");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 7);
    }

    #[test]
    fn parser_case_item_missing_close_paren() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(foo bar");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.case_item()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedPatternList)
        );
        assert_eq!(e.location.line.value, "(foo bar");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 6);
    }

    #[test]
    fn parser_case_command_minimum() {
        let mut lexer = Lexer::with_source(Source::Unknown, "case foo in esac");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::Case { subject, items } = result {
            assert_eq!(subject.to_string(), "foo");
            assert_eq!(items, []);
        } else {
            panic!("Not a case command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_newline_before_in() {
        // Alias substitution results in "case x \n\n \nin esac"
        let mut lexer = Lexer::with_source(Source::Unknown, "CASE_X IN_ESAC");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("".to_string());
        aliases.insert(HashEntry::new(
            "CASE_X".to_string(),
            " case x \n\n ".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "IN_ESAC".to_string(),
            "\nin esac".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let first_pass = block_on(parser.take_token_manual(true)).unwrap();
        assert!(first_pass.is_alias_substituted());

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::Case { subject, items } = result {
            assert_eq!(subject.to_string(), "x");
            assert_eq!(items, []);
        } else {
            panic!("Not a case command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_alias_on_subject() {
        // Alias substitution results in " case   in in  a|b) esac"
        let mut lexer = Lexer::with_source(Source::Unknown, "CASE in a|b) esac");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("".to_string());
        aliases.insert(HashEntry::new(
            "CASE".to_string(),
            " case ".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "in".to_string(),
            " in in ".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let first_pass = block_on(parser.take_token_manual(true)).unwrap();
        assert!(first_pass.is_alias_substituted());

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::Case { subject, items } = result {
            assert_eq!(subject.to_string(), "in");
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].to_string(), "(a | b) ;;");
        } else {
            panic!("Not a case command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_alias_on_in() {
        // Alias substitution results in "case x  in esac"
        let mut lexer = Lexer::with_source(Source::Unknown, "CASE_X in esac");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("".to_string());
        aliases.insert(HashEntry::new(
            "CASE_X".to_string(),
            "case x ".to_string(),
            false,
            origin.clone(),
        ));
        aliases.insert(HashEntry::new(
            "in".to_string(),
            "in a)".to_string(),
            false,
            origin,
        ));
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let first_pass = block_on(parser.take_token_manual(true)).unwrap();
        assert!(first_pass.is_alias_substituted());

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::Case { subject, items } = result {
            assert_eq!(subject.to_string(), "x");
            assert_eq!(items, []);
        } else {
            panic!("Not a case command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_one_item() {
        let mut lexer = Lexer::with_source(Source::Unknown, "case foo in bar) esac");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::Case { subject, items } = result {
            assert_eq!(subject.to_string(), "foo");
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].to_string(), "(bar) ;;");
        } else {
            panic!("Not a case command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_many_items_without_final_double_semicolon() {
        let mut lexer = Lexer::with_source(
            Source::Unknown,
            "case x in\n\na) ;; (b|c):&:; ;;\n d)echo\nesac",
        );
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::Case { subject, items } = result {
            assert_eq!(subject.to_string(), "x");
            assert_eq!(items.len(), 3);
            assert_eq!(items[0].to_string(), "(a) ;;");
            assert_eq!(items[1].to_string(), "(b | c) :& :;;");
            assert_eq!(items[2].to_string(), "(d) echo;;");
        } else {
            panic!("Not a case command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_many_items_with_final_double_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, "case x in(1);; 2)echo\n\n;;\n\nesac");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let CompoundCommand::Case { subject, items } = result {
            assert_eq!(subject.to_string(), "x");
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].to_string(), "(1) ;;");
            assert_eq!(items[1].to_string(), "(2) echo;;");
        } else {
            panic!("Not a case command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_missing_subject() {
        let mut lexer = Lexer::with_source(Source::Unknown, " case  ");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MissingCaseSubject));
        assert_eq!(e.location.line.value, " case  ");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 8);
    }

    #[test]
    fn parser_case_command_invalid_subject() {
        let mut lexer = Lexer::with_source(Source::Unknown, " case ; ");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidCaseSubject));
        assert_eq!(e.location.line.value, " case ; ");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 7);
    }

    #[test]
    fn parser_case_command_missing_in() {
        let mut lexer = Lexer::with_source(Source::Unknown, " case x esac");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::MissingIn { opening_location }) = e.cause {
            assert_eq!(opening_location.line.value, " case x esac");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 2);
        } else {
            panic!("Not a MissingIn: {:?}", e.cause);
        }
        assert_eq!(e.location.line.value, " case x esac");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 9);
    }

    #[test]
    fn parser_case_command_missing_esac() {
        let mut lexer = Lexer::with_source(Source::Unknown, "case x in a) }");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.compound_command()).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedCase { opening_location }) = e.cause {
            assert_eq!(opening_location.line.value, "case x in a) }");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("Not a MissingIn: {:?}", e.cause);
        }
        assert_eq!(e.location.line.value, "case x in a) }");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 14);
    }

    #[test]
    fn parser_compound_command_none() {
        let mut lexer = Lexer::with_source(Source::Unknown, "}");
        let mut parser = Parser::new(&mut lexer);

        let option = block_on(parser.compound_command()).unwrap();
        assert_eq!(option, None);
    }

    #[test]
    fn parser_full_compound_command_without_redirections() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(:)");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.full_compound_command()).unwrap().unwrap();
        let FullCompoundCommand { command, redirs } = result.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(command.to_string(), "(:)");
        assert_eq!(redirs, []);
    }

    #[test]
    fn parser_full_compound_command_with_redirections() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(command) <foo >bar ;");
        let mut parser = Parser::new(&mut lexer);

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
        let mut lexer = Lexer::with_source(Source::Unknown, "}");
        let mut parser = Parser::new(&mut lexer);

        let option = block_on(parser.full_compound_command()).unwrap();
        assert_eq!(option, None);
    }

    #[test]
    fn parser_short_function_definition_ok() {
        let mut lexer = Lexer::with_source(Source::Unknown, " ( ) ( : ) > /dev/null ");
        let mut parser = Parser::new(&mut lexer);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["foo".parse().unwrap()],
            redirs: vec![],
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

    #[test]
    fn parser_short_function_definition_not_one_word_name() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(");
        let mut parser = Parser::new(&mut lexer);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec![],
            redirs: vec![],
        };

        let result = block_on(parser.short_function_definition(c)).unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let Command::Simple(c) = result {
            assert_eq!(c.to_string(), "");
        } else {
            panic!("Not a simple command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, Operator(OpenParen));
    }

    #[test]
    fn parser_short_function_definition_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut parser = Parser::new(&mut lexer);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["foo".parse().unwrap()],
            redirs: vec![],
        };

        let result = block_on(parser.short_function_definition(c)).unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let Command::Simple(c) = result {
            assert_eq!(c.to_string(), "foo");
        } else {
            panic!("Not a simple command: {:?}", result);
        }
    }

    #[test]
    fn parser_short_function_definition_unmatched_parenthesis() {
        let mut lexer = Lexer::with_source(Source::Unknown, "( ");
        let mut parser = Parser::new(&mut lexer);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["foo".parse().unwrap()],
            redirs: vec![],
        };

        let e = block_on(parser.short_function_definition(c)).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::UnmatchedParenthesis)
        );
        assert_eq!(e.location.line.value, "( ");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 3);
    }

    #[test]
    fn parser_short_function_definition_missing_function_body() {
        let mut lexer = Lexer::with_source(Source::Unknown, "( ) ");
        let mut parser = Parser::new(&mut lexer);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["foo".parse().unwrap()],
            redirs: vec![],
        };

        let e = block_on(parser.short_function_definition(c)).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingFunctionBody)
        );
        assert_eq!(e.location.line.value, "( ) ");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 5);
    }

    #[test]
    fn parser_short_function_definition_invalid_function_body() {
        let mut lexer = Lexer::with_source(Source::Unknown, "() foo ; ");
        let mut parser = Parser::new(&mut lexer);
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["foo".parse().unwrap()],
            redirs: vec![],
        };

        let e = block_on(parser.short_function_definition(c)).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::InvalidFunctionBody)
        );
        assert_eq!(e.location.line.value, "() foo ; ");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 4);
    }

    #[test]
    fn parser_short_function_definition_close_parenthesis_alias() {
        let mut lexer = Lexer::with_source(Source::Unknown, " a b ");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("".to_string());
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
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let result = block_on(async {
            parser.simple_command().await.unwrap(); // alias
            let c = parser.simple_command().await.unwrap().unwrap().unwrap();
            parser.short_function_definition(c).await.unwrap()
        });
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let Command::Function(f) = result {
            assert_eq!(f.has_keyword, false);
            assert_eq!(f.name.to_string(), "f");
            assert_eq!(f.body.to_string(), "(:)");
        } else {
            panic!("Not a function definition: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_short_function_definition_body_alias_and_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, " a b ");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("".to_string());
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
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));

        let result = block_on(async {
            parser.simple_command().await.unwrap(); // alias
            let c = parser.simple_command().await.unwrap().unwrap().unwrap();
            parser.short_function_definition(c).await.unwrap()
        });
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let Command::Function(f) = result {
            assert_eq!(f.has_keyword, false);
            assert_eq!(f.name.to_string(), "f");
            assert_eq!(f.body.to_string(), "(:)");
        } else {
            panic!("Not a function definition: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_short_function_definition_alias_inapplicable() {
        let mut lexer = Lexer::with_source(Source::Unknown, "()b");
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("".to_string());
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
        let mut parser = Parser::with_aliases(&mut lexer, std::rc::Rc::new(aliases));
        let c = SimpleCommand {
            assigns: vec![],
            words: vec!["f".parse().unwrap()],
            redirs: vec![],
        };

        let e = block_on(parser.short_function_definition(c)).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::InvalidFunctionBody)
        );
        assert_eq!(e.location.line.value, "()b");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 3);
    }

    #[test]
    fn parser_command_simple() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo < bar");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.command()).unwrap().unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let Command::Simple(c) = result {
            assert_eq!(c.to_string(), "foo <bar");
        } else {
            panic!("Not a simple command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_command_compound() {
        let mut lexer = Lexer::with_source(Source::Unknown, "(foo) < bar");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.command()).unwrap().unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let Command::Compound(c) = result {
            assert_eq!(c.to_string(), "(foo) <bar");
        } else {
            panic!("Not a compound command: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_command_function() {
        let mut lexer = Lexer::with_source(Source::Unknown, "fun () ( echo )");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.command()).unwrap().unwrap().unwrap();
        let result = result.fill(&mut std::iter::empty()).unwrap();
        if let Command::Function(f) = result {
            assert_eq!(f.to_string(), "fun() (echo)");
        } else {
            panic!("Not a function definition: {:?}", result);
        }

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_command_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut parser = Parser::new(&mut lexer);

        let option = block_on(parser.command()).unwrap().unwrap();
        assert_eq!(option, None);
    }

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
        let origin = Location::dummy("".to_string());
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

    #[test]
    fn parser_and_or_list_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut parser = Parser::new(&mut lexer);

        let option = block_on(parser.and_or_list()).unwrap().unwrap();
        assert_eq!(option, None);
    }

    #[test]
    fn parser_and_or_list_one() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo");
        let mut parser = Parser::new(&mut lexer);

        let aol = block_on(parser.and_or_list()).unwrap().unwrap().unwrap();
        let aol = aol.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(aol.first.to_string(), "foo");
        assert_eq!(aol.rest, vec![]);
    }

    #[test]
    fn parser_and_or_list_many() {
        let mut lexer = Lexer::with_source(Source::Unknown, "first && second || \n\n third;");
        let mut parser = Parser::new(&mut lexer);

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
        let mut lexer = Lexer::with_source(Source::Unknown, "foo &&");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.and_or_list()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingPipeline(AndOr::AndThen))
        );
        assert_eq!(e.location.line.value, "foo &&");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 7);
    }

    #[test]
    fn parser_list_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.list()).unwrap().unwrap();
        assert_eq!(list.0, vec![]);
    }

    #[test]
    fn parser_list_one_item_without_last_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.list()).unwrap().unwrap();
        let list = list.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(list.0.len(), 1);
        assert_eq!(list.0[0].is_async, false);
        assert_eq!(list.0[0].and_or.to_string(), "foo");
    }

    #[test]
    fn parser_list_one_item_with_last_semicolon() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo;");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.list()).unwrap().unwrap();
        let list = list.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(list.0.len(), 1);
        assert_eq!(list.0[0].is_async, false);
        assert_eq!(list.0[0].and_or.to_string(), "foo");
    }

    #[test]
    fn parser_list_many_items() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo & bar ; baz&");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.list()).unwrap().unwrap();
        let list = list.fill(&mut std::iter::empty()).unwrap();
        assert_eq!(list.0.len(), 3);
        assert_eq!(list.0[0].is_async, true);
        assert_eq!(list.0[0].and_or.to_string(), "foo");
        assert_eq!(list.0[1].is_async, false);
        assert_eq!(list.0[1].and_or.to_string(), "bar");
        assert_eq!(list.0[2].is_async, true);
        assert_eq!(list.0[2].and_or.to_string(), "baz");
    }

    #[test]
    fn parser_command_line_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut parser = Parser::new(&mut lexer);

        let result = block_on(parser.command_line()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parser_command_line_command_and_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<END\nfoo\nEND\n");
        let mut parser = Parser::new(&mut lexer);

        let List(items) = block_on(parser.command_line()).unwrap().unwrap();
        assert_eq!(items.len(), 1);
        let item = items.first().unwrap();
        assert_eq!(item.is_async, false);
        let AndOrList { first, rest } = &item.and_or;
        assert!(rest.is_empty(), "expected empty rest: {:?}", rest);
        let Pipeline { commands, negation } = first;
        assert_eq!(*negation, false);
        assert_eq!(commands.len(), 1);
        let cmd = match commands[0] {
            Command::Simple(ref c) => c,
            _ => panic!("Expected a simple command but got {:?}", commands[0]),
        };
        assert_eq!(cmd.words, []);
        assert_eq!(cmd.redirs.len(), 1);
        assert_eq!(cmd.redirs[0].fd, None);
        if let RedirBody::HereDoc(ref here_doc) = cmd.redirs[0].body {
            let HereDoc {
                delimiter,
                remove_tabs,
                content,
            } = here_doc;
            assert_eq!(delimiter.to_string(), "END");
            assert_eq!(*remove_tabs, false);
            assert_eq!(content.to_string(), "foo\n");
        } else {
            panic!("Expected here-document, but got {:?}", cmd.redirs[0].body);
        }
    }

    #[test]
    fn parser_command_line_command_without_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo");
        let mut parser = Parser::new(&mut lexer);

        let cmd = block_on(parser.command_line()).unwrap().unwrap();
        assert_eq!(cmd.to_string(), "foo");
    }

    #[test]
    fn parser_command_line_newline_only() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\n");
        let mut parser = Parser::new(&mut lexer);

        let list = block_on(parser.command_line()).unwrap().unwrap();
        assert_eq!(list.0, []);
    }

    #[test]
    fn parser_command_line_here_doc_without_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<END");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.command_line()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingHereDocContent)
        );
        assert_eq!(e.location.line.value, "<<END");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 3);
    }

    #[test]
    fn parser_command_line_wrong_delimiter() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo)");
        let mut parser = Parser::new(&mut lexer);

        let e = block_on(parser.command_line()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::UnexpectedToken));
        assert_eq!(e.location.line.value, "foo)");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 4);
    }
}
