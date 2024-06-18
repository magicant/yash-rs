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
use crate::syntax::Redir;
use crate::syntax::Scalar;
use crate::syntax::SimpleCommand;
use crate::syntax::Word;

/// Simple command builder.
#[derive(Default)]
struct Builder {
    assigns: Vec<Assign>,
    words: Vec<Word>,
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

            // Tell assignment from word
            if !result.words.is_empty() {
                result.words.push(token.word);
                continue;
            }
            let mut assign = match Assign::try_from(token.word) {
                Ok(assign) => assign,
                Err(word) => {
                    result.words.push(word);
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

        Ok(Rec::Parsed(if result.is_empty() {
            None
        } else {
            Some(result.into())
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::ErrorCause;
    use super::super::lex::Lexer;
    use super::super::lex::TokenId::EndOfInput;
    use super::*;
    use crate::alias::EmptyGlossary;
    use crate::source::Source;
    use crate::syntax::RedirBody;
    use crate::syntax::RedirOp;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn parser_array_values_no_open_parenthesis() {
        let mut lexer = Lexer::from_memory(")", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);
        let result = parser.array_values().now_or_never().unwrap().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn parser_array_values_empty() {
        let mut lexer = Lexer::from_memory("()", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);
        let result = parser.array_values().now_or_never().unwrap();
        let words = result.unwrap().unwrap();
        assert_eq!(words, []);

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_array_values_many() {
        let mut lexer = Lexer::from_memory("(a b c)", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);
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
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);
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
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);
        let e = parser.array_values().now_or_never().unwrap().unwrap_err();
        assert_matches!(e.cause,
             ErrorCause::Syntax(SyntaxError::UnclosedArrayValue { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "(a b");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 0..1);
        });
        assert_eq!(*e.location.code.value.borrow(), "(a b");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 4..4);
    }

    #[test]
    fn parser_array_values_invalid_word() {
        let mut lexer = Lexer::from_memory("(a;b)", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);
        let e = parser.array_values().now_or_never().unwrap().unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedArrayValue { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "(a;b)");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 0..1);
        });
        assert_eq!(*e.location.code.value.borrow(), "(a;b)");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 2..3);
    }

    #[test]
    fn parser_simple_command_eof() {
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

        let result = parser.simple_command().now_or_never().unwrap();
        assert_eq!(result, Ok(Rec::Parsed(None)));
    }

    #[test]
    fn parser_simple_command_keyword() {
        let mut lexer = Lexer::from_memory("then", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

        let result = parser.simple_command().now_or_never().unwrap();
        assert_eq!(result, Ok(Rec::Parsed(None)));
    }

    #[test]
    fn parser_simple_command_one_assignment() {
        let mut lexer = Lexer::from_memory("my=assignment", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.words, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.assigns[0].name, "my");
        assert_eq!(sc.assigns[0].value.to_string(), "assignment");
        assert_eq!(*sc.assigns[0].location.code.value.borrow(), "my=assignment");
        assert_eq!(sc.assigns[0].location.code.start_line_number.get(), 1);
        assert_eq!(sc.assigns[0].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[0].location.range, 0..13);
    }

    #[test]
    fn parser_simple_command_many_assignments() {
        let mut lexer = Lexer::from_memory("a= b=! c=X", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.words, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.assigns.len(), 3);
        assert_eq!(sc.assigns[0].name, "a");
        assert_eq!(sc.assigns[0].value.to_string(), "");
        assert_eq!(*sc.assigns[0].location.code.value.borrow(), "a= b=! c=X");
        assert_eq!(sc.assigns[0].location.code.start_line_number.get(), 1);
        assert_eq!(sc.assigns[0].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[0].location.range, 0..2);
        assert_eq!(sc.assigns[1].name, "b");
        assert_eq!(sc.assigns[1].value.to_string(), "!");
        assert_eq!(*sc.assigns[1].location.code.value.borrow(), "a= b=! c=X");
        assert_eq!(sc.assigns[1].location.code.start_line_number.get(), 1);
        assert_eq!(sc.assigns[1].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[1].location.range, 3..6);
        assert_eq!(sc.assigns[2].name, "c");
        assert_eq!(sc.assigns[2].value.to_string(), "X");
        assert_eq!(*sc.assigns[2].location.code.value.borrow(), "a= b=! c=X");
        assert_eq!(sc.assigns[2].location.code.start_line_number.get(), 1);
        assert_eq!(sc.assigns[2].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[2].location.range, 7..10);
    }

    #[test]
    fn parser_simple_command_one_word() {
        let mut lexer = Lexer::from_memory("word", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.words.len(), 1);
        assert_eq!(sc.words[0].to_string(), "word");
    }

    #[test]
    fn parser_simple_command_many_words() {
        let mut lexer = Lexer::from_memory(": if then", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.words.len(), 3);
        assert_eq!(sc.words[0].to_string(), ":");
        assert_eq!(sc.words[1].to_string(), "if");
        assert_eq!(sc.words[2].to_string(), "then");
    }

    #[test]
    fn parser_simple_command_one_redirection() {
        let mut lexer = Lexer::from_memory("<foo", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

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
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

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
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.words.len(), 1);
        assert_eq!(sc.assigns[0].name, "if");
        assert_eq!(sc.assigns[0].value.to_string(), "then");
        assert_eq!(sc.words[0].to_string(), "else");
    }

    #[test]
    fn parser_simple_command_word_redirection() {
        let mut lexer = Lexer::from_memory("word <redirection", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns, []);
        assert_eq!(sc.words.len(), 1);
        assert_eq!(sc.redirs.len(), 1);
        assert_eq!(sc.words[0].to_string(), "word");
        assert_eq!(sc.redirs[0].fd, None);
        assert_matches!(sc.redirs[0].body, RedirBody::Normal { ref operator, ref operand } => {
            assert_eq!(operator, &RedirOp::FileIn);
            assert_eq!(operand.to_string(), "redirection")
        });
    }

    #[test]
    fn parser_simple_command_redirection_assignment() {
        let mut lexer = Lexer::from_memory("<foo a=b", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

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
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.words.len(), 1);
        assert_eq!(sc.redirs.len(), 1);
        assert_eq!(sc.assigns[0].name, "if");
        assert_eq!(sc.assigns[0].value.to_string(), "then");
        assert_eq!(sc.words[0].to_string(), "else");
        assert_eq!(sc.redirs[0].fd, None);
        assert_matches!(sc.redirs[0].body, RedirBody::Normal { ref operator, ref operand } => {
            assert_eq!(operator, &RedirOp::FileIn);
            assert_eq!(operand.to_string(), "foo")
        });
    }

    #[test]
    fn parser_simple_command_array_assignment() {
        let mut lexer = Lexer::from_memory("a=()", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

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
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.words, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.assigns[0].name, "a");
        assert_eq!(sc.assigns[0].value.to_string(), "");
        assert_eq!(*sc.assigns[0].location.code.value.borrow(), "a= ()");
        assert_eq!(sc.assigns[0].location.code.start_line_number.get(), 1);
        assert_eq!(sc.assigns[0].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[0].location.range, 0..2);

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, Operator(OpenParen));
    }

    #[test]
    fn parser_simple_command_non_empty_assignment_followed_by_parenthesis() {
        let mut lexer = Lexer::from_memory("a=b()", Source::Unknown);
        let mut parser = Parser::new(&mut lexer, &EmptyGlossary);

        let result = parser.simple_command().now_or_never().unwrap();
        let sc = result.unwrap().unwrap().unwrap();
        assert_eq!(sc.assigns.len(), 1);
        assert_eq!(sc.words, []);
        assert_eq!(*sc.redirs, []);
        assert_eq!(sc.assigns[0].name, "a");
        assert_eq!(sc.assigns[0].value.to_string(), "b");
        assert_eq!(*sc.assigns[0].location.code.value.borrow(), "a=b()");
        assert_eq!(sc.assigns[0].location.code.start_line_number.get(), 1);
        assert_eq!(sc.assigns[0].location.code.source, Source::Unknown);
        assert_eq!(sc.assigns[0].location.range, 0..3);

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, Operator(OpenParen));
    }
}
