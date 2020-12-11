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
