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

// TODO Should be a struct to include error Location.
/// Types of errors that may happen in parsing.
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    /// A here-document operator is missing its corresponding content.
    MissingHereDocContent,
}

impl fmt::Display for Error {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

/// Result of parsing.
pub type Result<T> = std::result::Result<T, Error>;

/// Placeholder for a here-document missing from the AST.
///
/// This appears in intermediate ASTs when a here-document operator has been parsed. Since the
/// content of the here-document appears apart from the operator in the source code, the final AST
/// cannot be produced until the content is parsed. The `MissingHereDoc` fills the missing part
/// in the intermediate ATS and is replaced with an actual [HereDoc] in the final step of parsing.
pub struct MissingHereDoc;

/// Partial AST that can be filled with missing parts to create the whole, final AST.
pub trait Fill<T = HereDoc> {
    /// Final AST created by filling `self`.
    type Full;
    /// Takes some items from the iterator to fill the missing parts of `self` to create the
    /// complete AST.
    fn fill(self, i: &mut dyn Iterator<Item = T>) -> Result<Self::Full>;
}

impl Fill for RedirBody<MissingHereDoc> {
    type Full = RedirBody;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<RedirBody> {
        match self {
            RedirBody::HereDoc(MissingHereDoc) => {
                let h = i.next().ok_or(Error::MissingHereDocContent)?;
                Ok(RedirBody::HereDoc(h))
            }
        }
    }
}

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
                    body: RedirBody::from(HereDoc {
                        delimiter: Word::with_str(tail),
                        remove_tabs: false,
                        content: Word::with_str(""),
                    }),
                })
            } else {
                words.push(Word::with_str(token))
            }
        }
        Ok(SimpleCommand { words, redirs })
        // TODO add redirections to waitlist
    }
}
