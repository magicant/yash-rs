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

//! Syntax parser for simple command

use super::core::Parser;
use super::core::Rec;
use super::core::Result;
use super::error::Error;
use super::error::SyntaxError;
use super::lex::Operator::{CloseParen, Newline, OpenParen};
use super::lex::TokenId::{Operator, Token};
use crate::syntax::Array;
use crate::syntax::Assign;
use crate::syntax::ExpansionMode;
use crate::syntax::MaybeLiteral as _;
use crate::syntax::Redir;
use crate::syntax::Scalar;
use crate::syntax::SimpleCommand;
use crate::syntax::Word;

/// Determines the expansion mode of a word.
///
/// TODO Elaborate
fn determine_expansion_mode(word: Word) -> (Word, ExpansionMode) {
    use crate::syntax::{TextUnit::Literal, WordUnit::Unquoted};
    if let Some(eq) = word.units.iter().position(|u| *u == Unquoted(Literal('='))) {
        if let Some(name) = word.units[..eq].to_string_if_literal() {
            if !name.is_empty() {
                let mut word = word;
                word.parse_tilde_everywhere_after(eq + 1);
                return (word, ExpansionMode::Single);
            }
        }
    }
    (word, ExpansionMode::Multiple)
}

/// Simple command builder.
#[derive(Default)]
struct Builder {
    assigns: Vec<Assign>,
    words: Vec<(Word, ExpansionMode)>,
    redirs: Vec<Redir>,
}

impl Builder {
    fn is_empty(&self) -> bool {
        self.assigns.is_empty() && self.words.is_empty() && self.redirs.is_empty()
    }
}

impl From<Builder> for SimpleCommand {
    fn from(builder: Builder) -> Self {
        SimpleCommand {
            assigns: builder.assigns,
            words: builder.words,
            redirs: builder.redirs.into(),
        }
    }
}

impl Parser<'_, '_> {
    /// Parses the value of an array assignment.
    ///
    /// This function first consumes a `(` token, then any number of words
    /// separated by blanks and/or newlines, and finally a `)`.
    /// If the first token is not `(`, the result is `Ok(None)`.
    /// If the last `)` is missing, the result is
    /// `Err(ErrorCause::Syntax(SyntaxError::UnclosedArrayValue(_)))`.
    pub async fn array_values(&mut self) -> Result<Option<Vec<Word>>> {
        if self.peek_token().await?.id != Operator(OpenParen) {
            return Ok(None);
        }

        let opening_location = self.take_token_raw().await?.word.location;
        let mut words = vec![];

        loop {
            let next = self.take_token_auto(&[]).await?;
            match next.id {
                Operator(Newline) => continue,
                Operator(CloseParen) => break,
                Token(_keyword) => words.push(next.word),
                _ => {
                    return Err(Error {
                        cause: SyntaxError::UnclosedArrayValue { opening_location }.into(),
                        location: next.word.location,
                    })
                }
            }
        }

        Ok(Some(words))
    }

    /// Parses a simple command.
    ///
    /// If there is no valid command at the current position, this function
    /// returns `Ok(Rec::Parsed(None))`.
    pub async fn simple_command(&mut self) -> Result<Rec<Option<SimpleCommand>>> {
        let mut is_declaration_utility = None;
        let mut result = Builder::default();

        loop {
            // Parse redirection
            if let Some(redir) = self.redirection().await? {
                result.redirs.push(redir);
                continue;
            }

            // Filter token type
            match self.peek_token().await?.id {
                Token(Some(_keyword)) if result.is_empty() => break,
                Token(_) => (),
                _ => break,
            }

            // Apply alias substitution
            let token = match self.take_token_manual(result.words.is_empty()).await? {
                Rec::AliasSubstituted => {
                    if result.is_empty() {
                        return Ok(Rec::AliasSubstituted);
                    } else {
                        continue;
                    }
                }
                Rec::Parsed(token) => token,
            };

            // Handle command argument word
            if let Some(is_declaration_utility) = is_declaration_utility {
                // The word determined (not) to be a declaration utility
                // must already be in the words list.
                debug_assert!(!result.words.is_empty());

                result.words.push(if is_declaration_utility {
                    determine_expansion_mode(token.word)
                } else {
                    (token.word, ExpansionMode::Multiple)
                });
                continue;
            }

            // Tell assignment from word
            let assign_or_word = if result.words.is_empty() {
                // We don't have any words yet, so this token may be an assignment or a word.
                Assign::try_from(token.word)
            } else {
                // We already have some words, so remaining tokens are all words.
                Err(token.word)
            };
            let mut assign = match assign_or_word {
                Ok(assign) => assign,
                Err(word) => {
                    debug_assert!(is_declaration_utility.is_none());
                    is_declaration_utility = self.word_names_declaration_utility(&word);
                    result.words.push((word, ExpansionMode::Multiple));
                    continue;
                }
            };

            let units = match &assign.value {
                Scalar(Word { units, .. }) => units,
                _ => panic!(
                    "Assign::try_from produced a non-scalar value {:?}",
                    assign.value
                ),
            };

            // Tell array assignment from scalar assignment
            // TODO no array assignment in POSIXly-correct mode
            if units.is_empty() && !self.has_blank().await? {
                if let Some(words) = self.array_values().await? {
                    assign.value = Array(words);
                }
            }

            result.assigns.push(assign);
        }

        Ok(Rec::Parsed((!result.is_empty()).then(|| result.into())))
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::lex::Lexer;
    use super::super::lex::TokenId::EndOfInput;
    use super::*;
    use crate::decl_util::EmptyGlossary;
    use crate::source::Source;
    use crate::syntax::RedirBody;
    use crate::syntax::RedirOp;
    use crate::syntax::TextUnit;
    use crate::syntax::WordUnit;
    use assert_matches::assert_matches;
    use futures_util::FutureExt as _;

    #[test]
    fn determine_expansion_mode_empty_name() {
        let in_word = "=".parse::<Word>().unwrap();
        let (out_word, mode) = determine_expansion_mode(in_word.clone());
        assert_eq!(out_word, in_word);
        assert_eq!(mode, ExpansionMode::Multiple);
    }

    #[test]
    fn determine_expansion_mode_nonempty_name() {
        let in_word = "foo=".parse::<Word>().unwrap();
        let (out_word, mode) = determine_expansion_mode(in_word.clone());
        assert_eq!(out_word, in_word);
        assert_eq!(mode, ExpansionMode::Single);
    }

    #[test]
    fn determine_expansion_mode_non_literal_name() {
        let in_word = "${X}=".parse::<Word>().unwrap();
        let (out_word, mode) = determine_expansion_mode(in_word.clone());
        assert_eq!(out_word, in_word);
        assert_eq!(mode, ExpansionMode::Multiple);
    }

    #[test]
    fn determine_expansion_mode_tilde_expansions_after_equal() {
        let word = "~=~:~b".parse().unwrap();
        let (word, mode) = determine_expansion_mode(word);
        assert_eq!(
            word.units,
            [
                WordUnit::Unquoted(TextUnit::Literal('~')),
                WordUnit::Unquoted(TextUnit::Literal('=')),
                WordUnit::Tilde("".to_string()),
                WordUnit::Unquoted(TextUnit::Literal(':')),
                WordUnit::Tilde("b".to_string()),
            ]
        );
        assert_eq!(mode, ExpansionMode::Single);
    }

    #[test]
    fn parser_array_values_no_open_parenthesis() {
        let mut lexer = Lexer::from_memory(")", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);
        let result = parser.array_values().now_or_never().unwrap().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn parser_array_values_empty() {
        let mut lexer = Lexer::from_memory("()", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);
        let result = parser.array_values().now_or_never().unwrap();
        let words = result.unwrap().unwrap();
        assert_eq!(words, []);

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_array_values_many() {
        let mut lexer = Lexer::from_memory("(a b c)", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);
        let result = parser.array_values().now_or_never().unwrap();
        let words = result.unwrap().unwrap();
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].to_string(), "a");
        assert_eq!(words[1].to_string(), "b");
        assert_eq!(words[2].to_string(), "c");
    }

    #[test]
    fn parser_array_values_newlines_and_comments() {
        let mut lexer = Lexer::from_memory(
            "(
            a # b
            c d
        )",
            Source::Unknown,
        );
        let mut parser = Parser::new(&mut lexer);
        let result = parser.array_values().now_or_never().unwrap();
        let words = result.unwrap().unwrap();
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].to_string(), "a");
        assert_eq!(words[1].to_string(), "c");
        assert_eq!(words[2].to_string(), "d");
    }

    #[test]
    fn parser_array_values_unclosed() {
        let mut lexer = Lexer::from_memory("(a b", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);
        let e = parser.array_values().now_or_never().unwrap().unwrap_err();
        assert_matches!(e.cause,
             ErrorCause::Syntax(SyntaxError::UnclosedArrayValue { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "(a b");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(*opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 0..1);
        });
        assert_eq!(*e.location.code.value.borrow(), "(a b");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 4..4);
    }

    #[test]
    fn parser_array_values_invalid_word() {
        let mut lexer = Lexer::from_memory("(a;b)", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);
        let e = parser.array_values().now_or_never().unwrap().unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedArrayValue { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "(a;b)");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(*opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 0..1);
        });
        assert_eq!(*e.location.code.value.borrow(), "(a;b)");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 2..3);
    }

    #[test]
    fn parser_simple_command_eof() {
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        assert_eq!(result, Ok(Rec::Parsed(None)));
    }

    #[test]
    fn parser_simple_command_keyword() {
        let mut lexer = Lexer::from_memory("then", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        assert_eq!(result, Ok(Rec::Parsed(None)));
    }

    #[test]
    fn parser_simple_command_one_assignment() {
        let mut lexer = Lexer::from_memory("my=assignment", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.words, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.assigns[0].name, "my");
        assert_eq!(sc.assigns[0].value.to_string(), "assignment");
        assert_eq!(*sc.assigns[0].location.code.value.borrow(), "my=assignment");
        assert_eq!(sc.assigns[0].location.code.start_line_number.get(), 1);
        assert_eq!(*sc.assigns[0].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[0].location.range, 0..13);
    }

    #[test]
    fn parser_simple_command_many_assignments() {
        let mut lexer = Lexer::from_memory("a= b=! c=X", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.words, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.assigns.len(), 3);
        assert_eq!(sc.assigns[0].name, "a");
        assert_eq!(sc.assigns[0].value.to_string(), "");
        assert_eq!(*sc.assigns[0].location.code.value.borrow(), "a= b=! c=X");
        assert_eq!(sc.assigns[0].location.code.start_line_number.get(), 1);
        assert_eq!(*sc.assigns[0].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[0].location.range, 0..2);
        assert_eq!(sc.assigns[1].name, "b");
        assert_eq!(sc.assigns[1].value.to_string(), "!");
        assert_eq!(*sc.assigns[1].location.code.value.borrow(), "a= b=! c=X");
        assert_eq!(sc.assigns[1].location.code.start_line_number.get(), 1);
        assert_eq!(*sc.assigns[1].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[1].location.range, 3..6);
        assert_eq!(sc.assigns[2].name, "c");
        assert_eq!(sc.assigns[2].value.to_string(), "X");
        assert_eq!(*sc.assigns[2].location.code.value.borrow(), "a= b=! c=X");
        assert_eq!(sc.assigns[2].location.code.start_line_number.get(), 1);
        assert_eq!(*sc.assigns[2].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[2].location.range, 7..10);
    }

    #[test]
    fn parser_simple_command_one_word() {
        let mut lexer = Lexer::from_memory("word", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.words.len(), 1);
        assert_eq!(sc.words[0].0.to_string(), "word");
        assert_eq!(sc.words[0].1, ExpansionMode::Multiple);
    }

    #[test]
    fn parser_simple_command_many_words() {
        let mut lexer = Lexer::from_memory(": if then", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.words.len(), 3);
        assert_eq!(sc.words[0].0.to_string(), ":");
        assert_eq!(sc.words[0].1, ExpansionMode::Multiple);
        assert_eq!(sc.words[1].0.to_string(), "if");
        assert_eq!(sc.words[1].1, ExpansionMode::Multiple);
        assert_eq!(sc.words[2].0.to_string(), "then");
        assert_eq!(sc.words[2].1, ExpansionMode::Multiple);
    }

    #[test]
    fn parser_simple_command_one_redirection() {
        let mut lexer = Lexer::from_memory("<foo", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(sc.words, []);
        assert_eq!(sc.redirs.len(), 1);
        assert_eq!(sc.redirs[0].fd, None);
        assert_matches!(sc.redirs[0].body, RedirBody::Normal { ref operator, ref operand } => {
            assert_eq!(operator, &RedirOp::FileIn);
            assert_eq!(operand.to_string(), "foo")
        });

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_simple_command_many_redirections() {
        let mut lexer = Lexer::from_memory("<one >two >>three", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(sc.words, []);
        assert_eq!(sc.redirs.len(), 3);
        assert_eq!(sc.redirs[0].fd, None);
        assert_matches!(sc.redirs[0].body, RedirBody::Normal { ref operator, ref operand } => {
            assert_eq!(operator, &RedirOp::FileIn);
            assert_eq!(operand.to_string(), "one")
        });
        assert_eq!(sc.redirs[1].fd, None);
        assert_matches!(sc.redirs[1].body, RedirBody::Normal { ref operator, ref operand } => {
            assert_eq!(operator, &RedirOp::FileOut);
            assert_eq!(operand.to_string(), "two")
        });
        assert_eq!(sc.redirs[2].fd, None);
        assert_matches!(sc.redirs[2].body, RedirBody::Normal { ref operator, ref operand } => {
            assert_eq!(operator, &RedirOp::FileAppend);
            assert_eq!(operand.to_string(), "three")
        });

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_simple_command_assignment_word() {
        let mut lexer = Lexer::from_memory("if=then else", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.words.len(), 1);
        assert_eq!(sc.assigns[0].name, "if");
        assert_eq!(sc.assigns[0].value.to_string(), "then");
        assert_eq!(sc.words[0].0.to_string(), "else");
        assert_eq!(sc.words[0].1, ExpansionMode::Multiple);
    }

    #[test]
    fn parser_simple_command_word_redirection() {
        let mut lexer = Lexer::from_memory("word <redirection", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(sc.words.len(), 1);
        assert_eq!(sc.redirs.len(), 1);
        assert_eq!(sc.words[0].0.to_string(), "word");
        assert_eq!(sc.words[0].1, ExpansionMode::Multiple);
        assert_eq!(sc.redirs[0].fd, None);
        assert_matches!(sc.redirs[0].body, RedirBody::Normal { ref operator, ref operand } => {
            assert_eq!(operator, &RedirOp::FileIn);
            assert_eq!(operand.to_string(), "redirection")
        });
    }

    #[test]
    fn parser_simple_command_redirection_assignment() {
        let mut lexer = Lexer::from_memory("<foo a=b", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.words, []);
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.redirs.len(), 1);
        assert_eq!(sc.assigns[0].name, "a");
        assert_eq!(sc.assigns[0].value.to_string(), "b");
        assert_eq!(sc.redirs[0].fd, None);
        assert_matches!(sc.redirs[0].body, RedirBody::Normal { ref operator, ref operand } => {
            assert_eq!(operator, &RedirOp::FileIn);
            assert_eq!(operand.to_string(), "foo")
        });
    }

    #[test]
    fn parser_simple_command_assignment_redirection_word() {
        let mut lexer = Lexer::from_memory("if=then <foo else", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.words.len(), 1);
        assert_eq!(sc.redirs.len(), 1);
        assert_eq!(sc.assigns[0].name, "if");
        assert_eq!(sc.assigns[0].value.to_string(), "then");
        assert_eq!(sc.words[0].0.to_string(), "else");
        assert_eq!(sc.words[0].1, ExpansionMode::Multiple);
        assert_eq!(sc.redirs[0].fd, None);
        assert_matches!(sc.redirs[0].body, RedirBody::Normal { ref operator, ref operand } => {
            assert_eq!(operator, &RedirOp::FileIn);
            assert_eq!(operand.to_string(), "foo")
        });
    }

    #[test]
    fn parser_simple_command_array_assignment() {
        let mut lexer = Lexer::from_memory("a=()", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.words, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.assigns[0].name, "a");
        assert_matches!(&sc.assigns[0].value, Array(words) => {
            assert_eq!(words, &[]);
        });

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_simple_command_empty_assignment_followed_by_blank_and_parenthesis() {
        let mut lexer = Lexer::from_memory("a= ()", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.words, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.assigns[0].name, "a");
        assert_eq!(sc.assigns[0].value.to_string(), "");
        assert_eq!(*sc.assigns[0].location.code.value.borrow(), "a= ()");
        assert_eq!(sc.assigns[0].location.code.start_line_number.get(), 1);
        assert_eq!(*sc.assigns[0].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[0].location.range, 0..2);

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, Operator(OpenParen));
    }

    #[test]
    fn parser_simple_command_non_empty_assignment_followed_by_parenthesis() {
        let mut lexer = Lexer::from_memory("a=b()", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.words, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.assigns[0].name, "a");
        assert_eq!(sc.assigns[0].value.to_string(), "b");
        assert_eq!(*sc.assigns[0].location.code.value.borrow(), "a=b()");
        assert_eq!(sc.assigns[0].location.code.start_line_number.get(), 1);
        assert_eq!(*sc.assigns[0].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[0].location.range, 0..3);

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, Operator(OpenParen));
    }

    #[test]
    fn word_with_single_expansion_mode_in_declaration_utility() {
        // "export" is a declaration utility, so the expansion mode of the word
        // "a=b" should be single.
        let mut lexer = Lexer::from_memory("export a=b", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(sc.words.len(), 2);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.words[0].0.to_string(), "export");
        assert_eq!(sc.words[0].1, ExpansionMode::Multiple);
        assert_eq!(sc.words[1].0.to_string(), "a=b");
        assert_eq!(sc.words[1].1, ExpansionMode::Single);
    }

    #[test]
    fn word_with_multiple_expansion_mode_in_declaration_utility() {
        // The expansion mode of the word "foo" should be multiple because it
        // cannot be parsed as an assignment.
        let mut lexer = Lexer::from_memory("export foo", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(sc.words.len(), 2);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.words[0].0.to_string(), "export");
        assert_eq!(sc.words[0].1, ExpansionMode::Multiple);
        assert_eq!(sc.words[1].0.to_string(), "foo");
        assert_eq!(sc.words[1].1, ExpansionMode::Multiple);
    }

    #[test]
    fn word_with_multiple_expansion_mode_in_non_declaration_utility() {
        // "foo" is not a declaration utility, so the expansion mode of the word
        // "a=b" should be multiple.
        let mut lexer = Lexer::from_memory("foo a=b", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(sc.words.len(), 2);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.words[0].0.to_string(), "foo");
        assert_eq!(sc.words[0].1, ExpansionMode::Multiple);
        assert_eq!(sc.words[1].0.to_string(), "a=b");
        assert_eq!(sc.words[1].1, ExpansionMode::Multiple);
    }

    #[test]
    fn declaration_utility_determined_by_non_first_word() {
        // "command" delegates to the next word to determine whether it is a
        // declaration utility.
        let mut lexer = Lexer::from_memory("command command export foo a=b", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);
        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.words[4].0.to_string(), "a=b");
        assert_eq!(sc.words[4].1, ExpansionMode::Single);

        let mut lexer = Lexer::from_memory("command command foo export a=b", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);
        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.words[4].0.to_string(), "a=b");
        assert_eq!(sc.words[4].1, ExpansionMode::Multiple);
    }

    #[test]
    fn no_declaration_utilities_with_empty_glossary() {
        // "export" is not a declaration utility in the empty glossary.
        let mut lexer = Lexer::from_memory("export a=b", Source::Unknown);
        let mut parser = Parser::config()
            .declaration_utilities(&EmptyGlossary)
            .input(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(sc.words.len(), 2);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.words[0].0.to_string(), "export");
        assert_eq!(sc.words[0].1, ExpansionMode::Multiple);
        assert_eq!(sc.words[1].0.to_string(), "a=b");
        assert_eq!(sc.words[1].1, ExpansionMode::Multiple);
    }

    #[test]
    fn custom_declaration_utility_glossary() {
        // "foo" is a declaration utility in the custom glossary.
        #[derive(Debug)]
        struct CustomGlossary;
        impl crate::decl_util::Glossary for CustomGlossary {
            fn is_declaration_utility(&self, name: &str) -> Option<bool> {
                Some(name == "foo")
            }
        }

        let mut lexer = Lexer::from_memory("foo a=b", Source::Unknown);
        let mut parser = Parser::config()
            .declaration_utilities(&CustomGlossary)
            .input(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(sc.words.len(), 2);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.words[0].0.to_string(), "foo");
        assert_eq!(sc.words[0].1, ExpansionMode::Multiple);
        assert_eq!(sc.words[1].0.to_string(), "a=b");
        assert_eq!(sc.words[1].1, ExpansionMode::Single);
    }

    #[test]
    fn assignment_is_not_considered_for_declaration_utility() {
        #[derive(Debug)]
        struct CustomGlossary;
        impl crate::decl_util::Glossary for CustomGlossary {
            fn is_declaration_utility(&self, _name: &str) -> Option<bool> {
                unreachable!("is_declaration_utility should not be called for assignments");
            }
        }

        let mut lexer = Lexer::from_memory("a=b", Source::Unknown);
        let mut parser = Parser::config()
            .declaration_utilities(&CustomGlossary)
            .input(&mut lexer);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.words, []);
        assert_eq!(*sc.redirs, [])
    }
}
