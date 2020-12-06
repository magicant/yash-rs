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

//! Shell command language syntax.
//!
//! TODO Elaborate

use itertools::Itertools;
use std::fmt;
use std::os::unix::io::RawFd;

/// Token that may involve expansion.
///
/// It depends on context whether an empty word is valid or not. It is your responsibility to
/// ensure a word is non-empty in a context where it cannot.
#[derive(Debug)]
pub struct Word(pub String); // TODO Redefine as a vector of word elements.

impl Word {
    /// Creates a constant word with unknown source.
    ///
    /// This is a convenience function to make a simple word, mainly for debugging
    /// purpose.
    ///
    /// The resulting word elements are not quoted even if they would be usually special.
    pub fn with_str(s: &str) -> Word {
        Word(s.to_string())
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Part of a redirection that defines the nature of the resulting file descriptor.
#[derive(Debug)]
pub enum RedirBody {
    // TODO filename-based redirections
    /// Here-document.
    HereDoc {
        /// Token that marks the end of the content of the here-document.
        delimiter: Word,

        /// Whether leading tab characters should be removed from each line of the
        /// here-document content. This value is `true` for the `<<-` operator and
        /// `false` for `<<`.
        remove_tabs: bool,

        /// Content of the here-document.
        ///
        /// The content ends with a newline unless it is empty. If the delimiter
        /// is quoted, the content must not contain any expansion.
        content: Word,
    },
}

impl fmt::Display for RedirBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RedirBody::HereDoc {
                delimiter,
                remove_tabs,
                ..
            } => {
                f.write_str(if *remove_tabs { "<<-" } else { "<<" })?;
                write!(f, "{}", *delimiter)
            }
        }
    }
}

/// Redirection.
#[derive(Debug)]
pub struct Redir {
    /// File descriptor that is modified by this redirection.
    pub fd: Option<RawFd>,
    /// Nature of the resulting file descriptor.
    pub body: RedirBody,
}

// TODO Should be somewhere else.
const STDIN_FD: RawFd = 0;
// const STDOUT_FD: RawFd = 1;

impl Redir {
    /// Computes the file descriptor that is modified by this redirection.
    ///
    /// If `self.fd` is `Some(_)`, the `RawFd` value is returned intact. Otherwise,
    /// the default file descriptor is selected depending on the type of `self.body`.
    pub fn fd_or_default(&self) -> RawFd {
        self.fd.unwrap_or_else(|| match self.body {
            RedirBody::HereDoc { .. } => STDIN_FD,
        })
    }
}

impl fmt::Display for Redir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(fd) = self.fd {
            write!(f, "{}", fd)?;
        }
        write!(f, "{}", self.body)
    }
}

/// Command that involves assignments, redirections, and word expansions.
///
/// In the shell language syntax, a valid simple command must contain at least one of assignments,
/// redirections, and words. The parser must not produce a completely empty simple command.
#[derive(Debug)]
pub struct SimpleCommand {
    pub words: Vec<Word>,
    pub redirs: Vec<Redir>,
    // TODO Assignments
}

impl fmt::Display for SimpleCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let i1 = self.words.iter().map(|x| x as &dyn fmt::Display);
        let i2 = self.redirs.iter().map(|x| x as &dyn fmt::Display);
        write!(f, "{}", i1.chain(i2).format(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redir_body_here_doc_display() {
        let heredoc = RedirBody::HereDoc {
            delimiter: Word::with_str("END"),
            remove_tabs: true,
            content: Word::with_str("here"),
        };
        assert_eq!(format!("{}", heredoc), "<<-END");

        let heredoc = RedirBody::HereDoc {
            delimiter: Word::with_str("XXX"),
            remove_tabs: false,
            content: Word::with_str("there"),
        };
        assert_eq!(format!("{}", heredoc), "<<XXX");
    }

    #[test]
    fn redir_display() {
        let heredoc = RedirBody::HereDoc {
            delimiter: Word::with_str("END"),
            remove_tabs: false,
            content: Word::with_str("here"),
        };

        let redir = Redir {
            fd: None,
            body: heredoc,
        };
        assert_eq!(format!("{}", redir), "<<END");
        let redir = Redir {
            fd: Some(0),
            ..redir
        };
        assert_eq!(format!("{}", redir), "0<<END");
        let redir = Redir {
            fd: Some(9),
            ..redir
        };
        assert_eq!(format!("{}", redir), "9<<END");
    }
}
