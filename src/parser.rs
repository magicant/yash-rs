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
    /// End of input is reached while more characters are expected to be read.
    EndOfInput,
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
/// content of the here-document appears apart from the operator in the source code, the complete
/// AST for the here-document cannot be produced when the operator has just been parsed. The
/// `MissingHereDoc` fills the missing part in the intermediate AST and is replaced with an
/// actual [HereDoc] after the content has been parsed.
pub struct MissingHereDoc;

/// Partial AST that can be filled with missing parts to create the whole, final AST.
pub trait Fill<T = HereDoc> {
    /// Final AST created by filling `self`.
    type Full;
    /// Takes some items from the iterator and fills the missing parts of `self` to create
    /// the complete AST.
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

impl Fill for Redir<MissingHereDoc> {
    type Full = Redir;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<Redir> {
        Ok(Redir {
            fd: self.fd,
            body: self.body.fill(i)?,
        })
    }
}

impl Fill for SimpleCommand<MissingHereDoc> {
    type Full = SimpleCommand;
    fn fill(mut self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<SimpleCommand> {
        let redirs = self.redirs.drain(..).try_fold(vec![], |mut vec, redir| {
            vec.push(redir.fill(i)?);
            Ok(vec)
        })?;
        Ok(SimpleCommand {
            words: self.words,
            redirs,
        })
    }
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
            Err(Error::EndOfInput)
        } else {
            Ok(Word(chars))
        }
    }

    /// Parses a simple command.
    pub async fn parse_simple_command(&mut self) -> Result<SimpleCommand> {
        let mut tokens = vec![];
        loop {
            let word = self.parse_word().await;
            if let Err(Error::EndOfInput) = word {
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
