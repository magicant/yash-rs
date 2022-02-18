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

//! Syntax parser for case command

use super::core::Parser;
use super::core::Rec;
use super::core::Result;
use super::error::Error;
use super::error::SyntaxError;
use super::lex::Keyword::{Case, Esac, In};
use super::lex::Operator::{Bar, CloseParen, Newline, OpenParen, SemicolonSemicolon};
use super::lex::TokenId::{self, EndOfInput, Operator, Token};
use crate::source::Location;
use crate::syntax::CaseItem;
use crate::syntax::CompoundCommand;

impl Parser<'_, '_> {
    /// Parses a case item.
    ///
    /// Does not parse the optional trailing double semicolon.
    ///
    /// Returns `None` if the next token is `esac`.
    pub async fn case_item(&mut self) -> Result<Option<CaseItem>> {
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
                        let location = Location {
                            code: next_token.word.span.code,
                            index: next_token.word.span.range.start,
                        };
                        return Err(Error { cause, location });
                    }
                }
            }
            _ => {
                let cause = pattern_error_cause(first_token.id).into();
                let location = Location {
                    code: first_token.word.span.code,
                    index: first_token.word.span.range.start,
                };
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
                            let location = Location {
                                code: pattern.word.span.code,
                                index: pattern.word.span.range.start,
                            };
                            return Err(Error { cause, location });
                        }
                    }
                }
                _ => {
                    let cause = SyntaxError::UnclosedPatternList.into();
                    let location = Location {
                        code: separator.word.span.code,
                        index: separator.word.span.range.start,
                    };
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
    pub async fn case_command(&mut self) -> Result<CompoundCommand> {
        let open = self.take_token_raw().await?;
        assert_eq!(open.id, Token(Some(Case)));

        let subject = self.take_token_auto(&[]).await?;
        match subject.id {
            Token(_) => (),
            Operator(Newline) | EndOfInput => {
                let cause = SyntaxError::MissingCaseSubject.into();
                let location = Location {
                    code: subject.word.span.code,
                    index: subject.word.span.range.start,
                };
                return Err(Error { cause, location });
            }
            _ => {
                let cause = SyntaxError::InvalidCaseSubject.into();
                let location = Location {
                    code: subject.word.span.code,
                    index: subject.word.span.range.start,
                };
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
                    let opening_location = Location {
                        code: open.word.span.code,
                        index: open.word.span.range.start,
                    };
                    let cause = SyntaxError::MissingIn { opening_location }.into();
                    let location = Location {
                        code: next_token.word.span.code,
                        index: next_token.word.span.range.start,
                    };
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
            let opening_location = Location {
                code: open.word.span.code,
                index: open.word.span.range.start,
            };
            let cause = SyntaxError::UnclosedCase { opening_location }.into();
            let location = Location {
                code: close.word.span.code,
                index: close.word.span.range.start,
            };
            return Err(Error { cause, location });
        }

        Ok(CompoundCommand::Case { subject, items })
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
    use futures_executor::block_on;

    #[test]
    fn parser_case_item_esac() {
        let mut lexer = Lexer::from_memory("\nESAC", Source::Unknown);
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
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
        let mut parser = Parser::new(&mut lexer, &aliases);

        let option = block_on(parser.case_item()).unwrap();
        assert_eq!(option, None);

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, Token(Some(Esac)));
    }

    #[test]
    fn parser_case_item_minimum() {
        let mut lexer = Lexer::from_memory("foo)", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let item = block_on(parser.case_item()).unwrap().unwrap();
        assert_eq!(item.patterns.len(), 1);
        assert_eq!(item.patterns[0].to_string(), "foo");
        assert_eq!(item.body.0, []);

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_item_with_open_paren() {
        let mut lexer = Lexer::from_memory("(foo)", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let item = block_on(parser.case_item()).unwrap().unwrap();
        assert_eq!(item.patterns.len(), 1);
        assert_eq!(item.patterns[0].to_string(), "foo");
        assert_eq!(item.body.0, []);

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_item_many_patterns() {
        let mut lexer = Lexer::from_memory("1 | esac | $three)", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

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
        let mut lexer = Lexer::from_memory("foo)\necho ok\n:&\n", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

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
        let mut lexer = Lexer::from_memory("foo);;", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let item = block_on(parser.case_item()).unwrap().unwrap();
        assert_eq!(item.patterns.len(), 1);
        assert_eq!(item.patterns[0].to_string(), "foo");
        assert_eq!(item.body.0, []);

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, Operator(SemicolonSemicolon));
    }

    #[test]
    fn parser_case_item_with_non_empty_body_and_double_semicolon() {
        let mut lexer = Lexer::from_memory("foo):;\n;;", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

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
        let mut lexer = Lexer::from_memory(")", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.case_item()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MissingPattern));
        assert_eq!(*e.location.code.value.borrow(), ")");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 0);
    }

    #[test]
    fn parser_case_item_esac_after_paren() {
        let mut lexer = Lexer::from_memory("(esac)", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.case_item()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EsacAsPattern));
        assert_eq!(*e.location.code.value.borrow(), "(esac)");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 1);
    }

    #[test]
    fn parser_case_item_first_pattern_not_word_after_open_paren() {
        let mut lexer = Lexer::from_memory("(&", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.case_item()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidPattern));
        assert_eq!(*e.location.code.value.borrow(), "(&");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 1);
    }

    #[test]
    fn parser_case_item_missing_pattern_after_bar() {
        let mut lexer = Lexer::from_memory("(foo| |", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.case_item()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MissingPattern));
        assert_eq!(*e.location.code.value.borrow(), "(foo| |");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 6);
    }

    #[test]
    fn parser_case_item_missing_close_paren() {
        let mut lexer = Lexer::from_memory("(foo bar", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.case_item()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedPatternList)
        );
        assert_eq!(*e.location.code.value.borrow(), "(foo bar");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 5);
    }

    #[test]
    fn parser_case_command_minimum() {
        let mut lexer = Lexer::from_memory("case foo in esac", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_matches!(result, CompoundCommand::Case { subject, items } => {
            assert_eq!(subject.to_string(), "foo");
            assert_eq!(items, []);
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_newline_before_in() {
        // Alias substitution results in "case x \n\n \nin esac"
        let mut lexer = Lexer::from_memory("CASE_X IN_ESAC", Source::Unknown);
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
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
        let mut parser = Parser::new(&mut lexer, &aliases);

        let first_pass = block_on(parser.take_token_manual(true)).unwrap();
        assert!(first_pass.is_alias_substituted());

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_matches!(result, CompoundCommand::Case { subject, items } => {
            assert_eq!(subject.to_string(), "x");
            assert_eq!(items, []);
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_alias_on_subject() {
        // Alias substitution results in " case   in in  a|b) esac"
        let mut lexer = Lexer::from_memory("CASE in a|b) esac", Source::Unknown);
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
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
        let mut parser = Parser::new(&mut lexer, &aliases);

        let first_pass = block_on(parser.take_token_manual(true)).unwrap();
        assert!(first_pass.is_alias_substituted());

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_matches!(result, CompoundCommand::Case { subject, items } => {
            assert_eq!(subject.to_string(), "in");
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].to_string(), "(a | b) ;;");
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_alias_on_in() {
        // Alias substitution results in "case x  in esac"
        let mut lexer = Lexer::from_memory("CASE_X in esac", Source::Unknown);
        let mut aliases = AliasSet::new();
        let origin = Location::dummy("");
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
        let mut parser = Parser::new(&mut lexer, &aliases);

        let first_pass = block_on(parser.take_token_manual(true)).unwrap();
        assert!(first_pass.is_alias_substituted());

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_matches!(result, CompoundCommand::Case { subject, items } => {
            assert_eq!(subject.to_string(), "x");
            assert_eq!(items, []);
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_one_item() {
        let mut lexer = Lexer::from_memory("case foo in bar) esac", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_matches!(result, CompoundCommand::Case { subject, items } => {
            assert_eq!(subject.to_string(), "foo");
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].to_string(), "(bar) ;;");
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_many_items_without_final_double_semicolon() {
        let mut lexer = Lexer::from_memory(
            "case x in\n\na) ;; (b|c):&:; ;;\n d)echo\nesac",
            Source::Unknown,
        );
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_matches!(result, CompoundCommand::Case { subject, items } => {
            assert_eq!(subject.to_string(), "x");
            assert_eq!(items.len(), 3);
            assert_eq!(items[0].to_string(), "(a) ;;");
            assert_eq!(items[1].to_string(), "(b | c) :& :;;");
            assert_eq!(items[2].to_string(), "(d) echo;;");
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_many_items_with_final_double_semicolon() {
        let mut lexer = Lexer::from_memory("case x in(1);; 2)echo\n\n;;\n\nesac", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let result = block_on(parser.compound_command()).unwrap().unwrap();
        assert_matches!(result, CompoundCommand::Case { subject, items } => {
            assert_eq!(subject.to_string(), "x");
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].to_string(), "(1) ;;");
            assert_eq!(items[1].to_string(), "(2) echo;;");
        });

        let next = block_on(parser.peek_token()).unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_case_command_missing_subject() {
        let mut lexer = Lexer::from_memory(" case  ", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MissingCaseSubject));
        assert_eq!(*e.location.code.value.borrow(), " case  ");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 7);
    }

    #[test]
    fn parser_case_command_invalid_subject() {
        let mut lexer = Lexer::from_memory(" case ; ", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidCaseSubject));
        assert_eq!(*e.location.code.value.borrow(), " case ; ");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 6);
    }

    #[test]
    fn parser_case_command_missing_in() {
        let mut lexer = Lexer::from_memory(" case x esac", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::MissingIn { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), " case x esac");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.index, 1);
        });
        assert_eq!(*e.location.code.value.borrow(), " case x esac");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 8);
    }

    #[test]
    fn parser_case_command_missing_esac() {
        let mut lexer = Lexer::from_memory("case x in a) }", Source::Unknown);
        let aliases = Default::default();
        let mut parser = Parser::new(&mut lexer, &aliases);

        let e = block_on(parser.compound_command()).unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedCase { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "case x in a) }");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.index, 0);
        });
        assert_eq!(*e.location.code.value.borrow(), "case x in a) }");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 13);
    }
}
