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

/// Partial result with possibly missing here-document operators.
///
/// The need for this type comes from the fact that a here-document operator has to be parsed in
/// two phases: The first parses the redirection operator token followed by the delimiter and the
/// second the content of the here-document. Since the content is apart from the operator in the
/// source code, the whole parse result cannot be created when the operator is just parsed. The
/// parser function returns a `Part` object in contexts where here-document contents may not yet
/// have been parsed. When the contents are eventually parsed, they must be passed to the `Part`
/// object then the whole final result is produced.
struct Part<T>(Box<dyn FnOnce(&mut dyn Iterator<Item = RedirBody>) -> T>);

impl<T> Part<T> {
    /// Creates a partial result with a function that will make the final result from given
    /// `RedirBody` objects.
    fn new<F>(f: F) -> Part<T>
    where
        F: FnOnce(&mut dyn Iterator<Item = RedirBody>) -> T + 'static,
    {
        Part(Box::new(f))
    }

    /// Creates the final result by filling the missing parts with the given `RedirBody` objects.
    fn fill(self, i: &mut dyn Iterator<Item = RedirBody>) -> T {
        self.0(i)
    }
}

/// Creates a result without any missing part.
///
/// The resulting `Part` object will just return `t` as the final result.
fn full<T: 'static>(t: T) -> Part<T> {
    Part(Box::new(|_| t))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dead_iter() -> impl Iterator<Item = RedirBody> {
        struct DeadIter;
        impl Iterator for DeadIter {
            type Item = RedirBody;
            fn next(&mut self) -> Option<RedirBody> {
                panic!("The dead iterator must not be consumed")
            }
        }
        DeadIter
    }

    #[test]
    fn part_full() {
        let p = full(42);
        assert_eq!(p.fill(&mut dead_iter()), 42);
    }

    #[test]
    fn part_single() {
        let p = Part::new(|r| r.next().unwrap());
        let mut v = vec![RedirBody::HereDoc {
            delimiter: Word::with_str("END"),
            remove_tabs: true,
            content: Word::with_str("foo"),
        }];
        let r = p.fill(&mut v.drain(..).chain(dead_iter()));

        assert!(v.is_empty());
        match r {
            RedirBody::HereDoc {
                delimiter,
                remove_tabs,
                content,
            } => {
                assert_eq!(format!("{}", delimiter), "END");
                assert!(remove_tabs);
                assert_eq!(format!("{}", content), "foo");
            } // , _ => panic!("Unexpected value {}", r),
        }
    }
}
