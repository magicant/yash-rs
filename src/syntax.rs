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
//! This module contains types that represent abstract syntax trees (ASTs) of the shell language.
//!
//! Some types in this module has the type parameter `<H = HereDoc>`. As a user of the AST, you
//! will never have to specify the parameter other than the default `HereDoc`. The parameter is
//! used by the parser to create intermediate ASTs that lack sub-trees for here-documents, since
//! the contents of here-documents have to be parsed separately from the normal flow of source code.
//!
//! TODO Elaborate

use crate::source::Location;
use itertools::Itertools;
use std::fmt;
use std::os::unix::io::RawFd;
use std::rc::Rc;

/// Element of a [Word] that can be double-quoted.
#[derive(Clone, Debug)]
pub enum DoubleQuotable {
    /// Literal single character.
    Literal(char),
    /// Backslash-escaped single character.
    Backslashed(char),
    // Parameter(TODO),
    /// Command substitution of the form `$(...)`.
    CommandSubst { content: String, location: Location },
    // Backquote(TODO),
    // Arith(TODO),
}

pub use DoubleQuotable::*;

impl fmt::Display for DoubleQuotable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal(c) => write!(f, "{}", c),
            Backslashed(c) => write!(f, "\\{}", c),
            CommandSubst { content, .. } => write!(f, "$({})", content),
        }
    }
}

impl PartialEq for DoubleQuotable {
    fn eq(&self, other: &DoubleQuotable) -> bool {
        match self {
            Literal(c1) => matches!(other, Literal(c2) if c1 == c2),
            Backslashed(c1) => matches!(other, Backslashed(c2) if c1 == c2),
            CommandSubst { content: c1, .. } => {
                matches!(other, CommandSubst{content:c2,..} if c1 == c2)
            }
        }
    }
}

/// Element of a [Word].
#[derive(Clone, Debug, PartialEq)]
pub enum WordUnit {
    /// Unquoted [`DoubleQuotable`] as a word unit.
    Unquoted(DoubleQuotable),
    // TODO DoubleQuote(Vec<DoubleQuotable>),
    // TODO SingleQuote(String),
}

pub use WordUnit::*;

impl fmt::Display for WordUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unquoted(ref dq) => write!(f, "{}", dq),
        }
    }
}

/// Token that may involve expansion.
///
/// It depends on context whether an empty word is valid or not. It is your responsibility to
/// ensure a word is non-empty in a context where it cannot.
#[derive(Clone, Debug)]
pub struct Word {
    /// Word units that constitute the word.
    pub units: Vec<WordUnit>,
    /// Location of the first character of the word.
    pub location: Location,
}

impl Word {
    /// Creates a constant word with unknown source.
    ///
    /// This is a convenience function to make a simple word, mainly for debugging
    /// purpose.
    ///
    /// The resulting word elements are not quoted even if they would be usually special.
    pub fn with_str(s: String) -> Word {
        let mut units = vec![];
        for c in s.chars() {
            units.push(WordUnit::Unquoted(DoubleQuotable::Literal(c)));
        }
        Word {
            units,
            location: Location::dummy(s),
        }
    }

    /// Converts the word to a string if the word is fully literal, that is, all composed of
    /// `WordUnit::Unquoted(DoubleQuotable::Literal(_))`.
    pub fn to_string_if_literal(&self) -> Option<String> {
        fn try_to_char(u: &WordUnit) -> Option<char> {
            if let Unquoted(Literal(c)) = u {
                Some(*c)
            } else {
                None
            }
        }
        self.units.iter().map(try_to_char).collect()
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.units.iter().try_for_each(|unit| write!(f, "{}", unit))
    }
}

/// Here-document.
#[derive(Clone, Debug)]
pub struct HereDoc {
    /// Token that marks the end of the content of the here-document.
    pub delimiter: Word,

    /// Whether leading tab characters should be removed from each line of the
    /// here-document content. This value is `true` for the `<<-` operator and
    /// `false` for `<<`.
    pub remove_tabs: bool,

    /// Content of the here-document.
    ///
    /// The content ends with a newline unless it is empty. If the delimiter
    /// is quoted, the content must not contain any expansion.
    pub content: Word,
}

impl fmt::Display for HereDoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.remove_tabs { "<<-" } else { "<<" })?;
        write!(f, "{}", self.delimiter)
    }
}

/// Part of a redirection that defines the nature of the resulting file descriptor.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RedirBody<H = HereDoc> {
    // TODO filename-based redirections
    /// Here-document.
    HereDoc(H),
}

impl fmt::Display for RedirBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RedirBody::HereDoc(h) => write!(f, "{}", h),
        }
    }
}

impl From<HereDoc> for RedirBody {
    fn from(h: HereDoc) -> Self {
        RedirBody::HereDoc(h)
    }
}

/// Redirection.
#[derive(Clone, Debug)]
pub struct Redir<H = HereDoc> {
    /// File descriptor that is modified by this redirection.
    pub fd: Option<RawFd>,
    /// Nature of the resulting file descriptor.
    pub body: RedirBody<H>,
}

// TODO Should be somewhere else.
const STDIN_FD: RawFd = 0;
// const STDOUT_FD: RawFd = 1;

impl<H> Redir<H> {
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
#[derive(Clone, Debug)]
pub struct SimpleCommand<H = HereDoc> {
    pub words: Vec<Word>,
    pub redirs: Vec<Redir<H>>,
    // TODO Assignments
}

impl fmt::Display for SimpleCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let i1 = self.words.iter().map(|x| x as &dyn fmt::Display);
        let i2 = self.redirs.iter().map(|x| x as &dyn fmt::Display);
        write!(f, "{}", i1.chain(i2).format(" "))
    }
}

/// Element of a pipe sequence.
#[derive(Clone, Debug)]
pub enum Command<H = HereDoc> {
    /// Simple command.
    SimpleCommand(SimpleCommand<H>),
    // TODO Compound command
    // TODO Function definition
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::SimpleCommand(c) => write!(f, "{}", c),
        }
    }
}

/// Commands separated by `|`
#[derive(Clone, Debug)]
pub struct Pipeline<H = HereDoc> {
    /// Elements of the pipeline.
    ///
    /// A valid pipeline must have at least one command.
    pub commands: Vec<Rc<Command<H>>>,
    /// True if the pipeline begins with a `!`.
    pub negation: bool,
}

impl fmt::Display for Pipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        if self.negation {
            write!(f, "! ")?;
        }
        write!(f, "{}", self.commands.iter().format(" | "))
    }
}

/// Condition that decides if a [Pipeline] in an [and-or list](AndOrList) should be executed.
#[derive(Clone, Copy, Debug)]
pub enum AndOr {
    /// `&&`
    AndThen,
    /// `||`
    OrElse,
}

impl fmt::Display for AndOr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AndOr::AndThen => write!(f, "&&"),
            AndOr::OrElse => write!(f, "||"),
        }
    }
}

/// Pipelines separated by `&&` and `||`.
#[derive(Clone, Debug)]
pub struct AndOrList<H = HereDoc> {
    pub first: Pipeline<H>,
    pub rest: Vec<(AndOr, Pipeline<H>)>,
}

impl fmt::Display for AndOrList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.first)?;
        self.rest
            .iter()
            .try_for_each(|(c, p)| write!(f, " {} {}", c, p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_quotable_display() {
        let literal = Literal('A');
        assert_eq!(literal.to_string(), "A");
        let backslashed = Backslashed('X');
        assert_eq!(backslashed.to_string(), r"\X");
    }

    #[test]
    fn word_to_string_if_literal_success() {
        let empty = Word::with_str("".to_string());
        let s = empty.to_string_if_literal().unwrap();
        assert_eq!(s, "");

        let nonempty = Word::with_str("foo".to_string());
        let s = nonempty.to_string_if_literal().unwrap();
        assert_eq!(s, "foo");
    }

    #[test]
    fn word_to_string_if_literal_failure() {
        let location = Location::dummy("foo".to_string());
        let backslashed = Unquoted(Backslashed('?'));
        let word = Word {
            units: vec![backslashed],
            location,
        };
        assert_eq!(word.to_string_if_literal(), None);
    }

    #[test]
    fn here_doc_display() {
        let heredoc = HereDoc {
            delimiter: Word::with_str("END".to_string()),
            remove_tabs: true,
            content: Word::with_str("here".to_string()),
        };
        assert_eq!(format!("{}", heredoc), "<<-END");

        let heredoc = HereDoc {
            delimiter: Word::with_str("XXX".to_string()),
            remove_tabs: false,
            content: Word::with_str("there".to_string()),
        };
        assert_eq!(format!("{}", heredoc), "<<XXX");
    }

    #[test]
    fn redir_display() {
        let heredoc = HereDoc {
            delimiter: Word::with_str("END".to_string()),
            remove_tabs: false,
            content: Word::with_str("here".to_string()),
        };

        let redir = Redir {
            fd: None,
            body: heredoc.into(),
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

    #[test]
    fn simple_command_display() {
        let mut command = SimpleCommand {
            words: vec![],
            redirs: vec![],
        };
        assert_eq!(format!("{}", command), "");

        command.words.push(Word::with_str("echo".to_string()));
        assert_eq!(format!("{}", command), "echo");

        command.words.push(Word::with_str("foo".to_string()));
        assert_eq!(format!("{}", command), "echo foo");

        command.redirs.push(Redir {
            fd: None,
            body: RedirBody::from(HereDoc {
                delimiter: Word::with_str("END".to_string()),
                remove_tabs: false,
                content: Word::with_str("".to_string()),
            }),
        });
        assert_eq!(format!("{}", command), "echo foo <<END");

        command.words.clear();
        assert_eq!(format!("{}", command), "<<END");

        command.redirs.push(Redir {
            fd: Some(1),
            body: RedirBody::from(HereDoc {
                delimiter: Word::with_str("here".to_string()),
                remove_tabs: true,
                content: Word::with_str("ignored".to_string()),
            }),
        });
        assert_eq!(format!("{}", command), "<<END 1<<-here");

        // TODO Assignments
    }

    fn dummy_command(s: String) -> Rc<Command> {
        let w = Word::with_str(s);
        let s = SimpleCommand {
            words: vec![w],
            redirs: vec![],
        };
        Rc::new(Command::SimpleCommand(s))
    }

    fn dummy_pipeline(s: String) -> Pipeline {
        let c = dummy_command(s);
        Pipeline {
            commands: vec![c],
            negation: false,
        }
    }

    #[test]
    fn pipeline_display() {
        let mut p = Pipeline {
            commands: vec![],
            negation: false,
        };
        p.commands.push(dummy_command("first".to_string()));
        assert_eq!(p.to_string(), "first");

        p.negation = true;
        assert_eq!(p.to_string(), "! first");

        p.commands.push(dummy_command("second".to_string()));
        assert_eq!(p.to_string(), "! first | second");

        p.commands.push(dummy_command("third".to_string()));
        p.negation = false;
        assert_eq!(p.to_string(), "first | second | third");
    }

    #[test]
    fn and_or_list_display() {
        let p = dummy_pipeline("first".to_string());
        let mut aol = AndOrList {
            first: p,
            rest: vec![],
        };
        assert_eq!(aol.to_string(), "first");

        let p = dummy_pipeline("second".to_string());
        aol.rest.push((AndOr::AndThen, p));
        assert_eq!(aol.to_string(), "first && second");

        let p = dummy_pipeline("third".to_string());
        aol.rest.push((AndOr::OrElse, p));
        assert_eq!(aol.to_string(), "first && second || third");
    }
}
