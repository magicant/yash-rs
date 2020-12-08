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

use super::source::*;
use super::syntax::*;
use std::fmt;
use std::num::NonZeroU64;
use std::rc::Rc;

/// Types of errors that may happen in parsing.
#[derive(Debug, Eq, PartialEq)]
pub enum Error {}

impl fmt::Display for Error {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

/// Result of parsing.
pub type Result<T> = std::result::Result<T, Error>;

/// Set of intermediate data used in parsing.
pub struct Parser {
    source: Vec<SourceChar>,
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
        }
    }

    /// Parses a simple command.
    pub async fn parse_simple_command(&mut self) -> Result<SimpleCommand> {
        let s = self.source.iter().map(|sc| sc.value).collect::<String>();
        let mut words = vec![];
        let mut redirs = vec![];
        for token in s.split_whitespace() {
            if let Some(tail) = token.strip_prefix("<<") {
                redirs.push(Redir {
                    fd: None,
                    body: RedirBody::HereDoc {
                        delimiter: Word::with_str(tail),
                        remove_tabs: false,
                        content: Word::with_str(""),
                    },
                })
            } else {
                words.push(Word::with_str(token))
            }
        }
        Ok(SimpleCommand { words, redirs })
        // TODO add redirections to waitlist
    }
}
