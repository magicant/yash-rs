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

use super::source::*;
use super::syntax::*;
use std::num::NonZeroU64;
use std::rc::Rc;

pub use self::core::Error;
pub use self::core::ErrorCause;
pub use self::core::Result;

// TODO remove dummy location and use actual locations
fn dummy_location() -> Location {
    let value = "".to_string();
    let number = NonZeroU64::new(1).unwrap();
    let source = Source::Unknown;
    let line = Rc::new(Line {
        value,
        number,
        source,
    });
    let column = number;
    Location { line, column }
}

pub use self::core::Lexer;

impl Lexer {
    /// Skips a character if the given function returns true for it.
    pub async fn skip_if<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(char) -> bool,
    {
        matches!(self.next_if(f).await, Ok(Some(_)))
    }

    /// Skips blank characters until reaching a non-blank.
    pub async fn skip_blanks(&mut self) {
        // TODO Support locale-dependent decision
        while self.skip_if(|c| c != '\n' && c.is_whitespace()).await {}
    }

    /// Skips a comment, if any.
    ///
    /// A comment ends just before a newline. The newline is *not* part of the comment.
    pub async fn skip_comment(&mut self) {
        if !self.skip_if(|c| c == '#').await {
            return;
        }

        while self.skip_if(|c| c != '\n').await {}
    }

    /// Skips blank characters and a comment, if any.
    pub async fn skip_blanks_and_comment(&mut self) {
        self.skip_blanks().await;
        self.skip_comment().await;
    }
}

pub use self::core::Parser as Parser2; // TODO

/// Set of intermediate data used in parsing.
pub struct Parser {
    source: Vec<SourceChar>,
    index: usize,
}

impl Parser {
    /// Creates a new parser.
    pub fn new(input: String) -> Parser {
        let line = Line {
            value: input,
            number: NonZeroU64::new(1).unwrap(),
            source: Source::Unknown,
        };
        Parser {
            source: Rc::new(line).enumerate().collect(),
            index: 0,
        }
    }

    /// Parses a word token.
    pub async fn parse_word(&mut self) -> Result<Word> {
        while self.index < self.source.len() && self.source[self.index].value.is_whitespace() {
            self.index += 1;
        }

        let mut chars = String::new();
        while self.index < self.source.len() && !self.source[self.index].value.is_whitespace() {
            chars.push(self.source[self.index].value);
            self.index += 1;
        }

        if chars.is_empty() {
            // TODO Report the actual location
            Err(Error {
                cause: ErrorCause::EndOfInput,
                location: dummy_location(),
            })
        } else {
            Ok(Word(chars))
        }
    }

    /// Parses a simple command.
    pub async fn parse_simple_command(&mut self) -> Result<SimpleCommand> {
        let mut tokens = vec![];
        loop {
            let word = self.parse_word().await;
            if let Err(Error {
                cause: ErrorCause::EndOfInput,
                ..
            }) = word
            {
                break;
            }
            tokens.push(word?);
        }
        let mut words = vec![];
        let mut redirs = vec![];
        for token in tokens {
            if let Some(tail) = token.0.strip_prefix("<<") {
                redirs.push(Redir {
                    fd: None,
                    body: RedirBody::from(HereDoc {
                        delimiter: Word::with_str(tail),
                        remove_tabs: false,
                        content: Word::with_str(""),
                    }),
                })
            } else {
                words.push(token)
            }
        }
        Ok(SimpleCommand { words, redirs })
        // TODO add redirections to waitlist
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexer_skip_blanks() {
        let mut runner = futures::executor::LocalPool::new();
        let mut lexer = Lexer::with_source(Source::Unknown, " \t w");

        let c = runner
            .run_until(async {
                lexer.skip_blanks().await;
                lexer.peek().await
            })
            .unwrap();
        assert_eq!(c.value, 'w');
        assert_eq!(c.location.line.value, " \t w");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 4);

        // Test idempotence
        let c = runner
            .run_until(async {
                lexer.skip_blanks().await;
                lexer.peek().await
            })
            .unwrap();
        assert_eq!(c.value, 'w');
        assert_eq!(c.location.line.value, " \t w");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 4);
    }

    #[test]
    fn lexer_skip_blanks_does_not_skip_newline() {
        let mut runner = futures::executor::LocalPool::new();
        let mut lexer = Lexer::with_source(Source::Unknown, "\n");

        let (c1, c2) = runner.run_until(async {
            let c1 = lexer.peek().await;
            lexer.skip_blanks().await;
            let c2 = lexer.peek().await;
            (c1, c2)
        });
        assert_eq!(c1, c2);
    }

    #[test]
    fn lexer_skip_comment_no_comment() {
        let mut runner = futures::executor::LocalPool::new();
        let mut lexer = Lexer::with_source(Source::Unknown, "\n");

        let (c1, c2) = runner.run_until(async {
            let c1 = lexer.peek().await;
            lexer.skip_comment().await;
            let c2 = lexer.peek().await;
            (c1, c2)
        });
        assert_eq!(c1, c2);
    }

    #[test]
    fn lexer_skip_comment_empty_comment() {
        let mut runner = futures::executor::LocalPool::new();
        let mut lexer = Lexer::with_source(Source::Unknown, "#\n");

        let c = runner
            .run_until(async {
                lexer.skip_comment().await;
                lexer.peek().await
            })
            .unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.line.value, "#\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 2);

        // Test idempotence
        let c = runner
            .run_until(async {
                lexer.skip_comment().await;
                lexer.peek().await
            })
            .unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.line.value, "#\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 2);
    }

    #[test]
    fn lexer_skip_comment_non_empty_comment() {
        let mut runner = futures::executor::LocalPool::new();
        let mut lexer = Lexer::with_source(Source::Unknown, "### foo bar\\\n");

        let c = runner
            .run_until(async {
                lexer.skip_comment().await;
                lexer.peek().await
            })
            .unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.line.value, "### foo bar\\\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 13);

        // Test idempotence
        let c = runner
            .run_until(async {
                lexer.skip_comment().await;
                lexer.peek().await
            })
            .unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.line.value, "### foo bar\\\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 13);
    }

    #[test]
    fn lexer_skip_comment_not_ending_with_newline() {
        let mut runner = futures::executor::LocalPool::new();
        let mut lexer = Lexer::with_source(Source::Unknown, "#comment");

        let e = runner
            .run_until(async {
                lexer.skip_comment().await;
                lexer.peek().await
            })
            .unwrap_err();
        assert_eq!(e.cause, ErrorCause::EndOfInput);
        assert_eq!(e.location.line.value, "#comment");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 9);

        // Test idempotence
        let e = runner
            .run_until(async {
                lexer.skip_comment().await;
                lexer.peek().await
            })
            .unwrap_err();
        assert_eq!(e.cause, ErrorCause::EndOfInput);
        assert_eq!(e.location.line.value, "#comment");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 9);
    }
}
