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

//! Fundamentals for implementing the parser.
//!
//! This module includes common types that are used as building blocks for constructing the syntax
//! parser.

use crate::source::Line;
use crate::source::Location;
use crate::source::SourceChar;
use std::fmt;
use std::future::Future;
use std::future::ready;
use std::num::NonZeroU64;

/// Types of errors that may happen in parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorCause {
    /// End of input is reached while more characters are expected to be read.
    EndOfInput,
    // TODO Include the corresponding here-doc operator.
    /// A here-document operator is missing its corresponding content.
    MissingHereDocContent,
}

impl fmt::Display for ErrorCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCause::EndOfInput => f.write_str("Incomplete command"),
            ErrorCause::MissingHereDocContent => {
                f.write_str("Content of the here-document is missing")
            }
        }
    }
}

/// Explanation of a failure in parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub cause: ErrorCause,
    pub location: Location,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
        // TODO Print Location
    }
}

/// Entire result of parsing.
pub type Result<T> = std::result::Result<T, Error>;

/// Modifier that makes a result of parsing optional to trigger the parser to restart after alias
/// substitution.
///
/// `Rec` stands for "recursion", as its method allows automatic recursion of parsers.
#[derive(Debug, Eq, PartialEq)]
pub enum Rec<T> {
    /// Result of alias substitution.
    ///
    /// After alias substitution occurred, the substituted source code has to be parsed by the
    /// parser that caused the alias substitution.
    AliasSubstituted,
    /// Successful result that was produced without consuming any input characters.
    Empty(T),
    /// Successful result that was produced by consuming one or more input characters.
    NonEmpty(T),
}

/// Repeatedly applies the parser that may involve alias substitution until the final result is
/// obtained.
pub fn finish<T, F>(mut f: F) -> Result<T>
where
    F: FnMut() -> Result<Rec<T>>,
{
    loop {
        if let Rec::Empty(t) | Rec::NonEmpty(t) = f()? {
            return Ok(t);
        }
    }
}

impl<T> Rec<T> {
    /// Combines `self` with another parser.
    ///
    /// If `self` is `AliasSubstituted`, `zip` returns `AliasSubstituted` without calling `f`.
    /// Otherwise, `f` is called with the result contained in `self`. If `self` is `NonEmpty`, `f`
    /// is called as many times until it returns a result that is not `AliasSubstituted`. Lastly,
    /// the values of the two `Rec` objects are merged into one.
    pub fn zip<U, F>(self, mut f: F) -> Result<Rec<(T, U)>>
    where
        F: FnMut(&T) -> Result<Rec<U>>,
    {
        match self {
            Rec::AliasSubstituted => Ok(Rec::AliasSubstituted),
            Rec::Empty(t) => match f(&t)? {
                Rec::AliasSubstituted => Ok(Rec::AliasSubstituted),
                Rec::Empty(u) => Ok(Rec::Empty((t, u))),
                Rec::NonEmpty(u) => Ok(Rec::NonEmpty((t, u))),
            },
            Rec::NonEmpty(t) => {
                let u = finish(|| f(&t))?;
                Ok(Rec::NonEmpty((t, u)))
            }
        }
    }

    /// Transforms the result value in `self`.
    pub fn map<U, F>(self, f: F) -> Result<Rec<U>>
    where
        F: FnOnce(T) -> Result<U>,
    {
        match self {
            Rec::AliasSubstituted => Ok(Rec::AliasSubstituted),
            Rec::Empty(t) => Ok(Rec::Empty(f(t)?)),
            Rec::NonEmpty(t) => Ok(Rec::NonEmpty(f(t)?)),
        }
    }
}

/// Current state in which input is read.
///
/// The context is passed to the input function so that it can read the input in a
/// context-dependent way.
///
/// Currently, this structure is empty. It may be extended to provide with some useful data in
/// future versions.
#[derive(Debug)]
pub struct Context;

/// Set of data used in lexical parsing.
pub struct Lexer {
    input: Box<dyn FnMut(&Context) -> Box<dyn Future<Output = Result<Line>>>>,
    source: Vec<SourceChar>,
    index: usize,
}

impl fmt::Debug for Lexer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        f.debug_struct("Lexer")
            .field("source", &self.source)
            .field("index", &self.index)
            .finish()
        // TODO Call finish_non_exhaustive instead of finish
    }
}

/// Set of data used in syntax parsing.
#[derive(Debug)]
pub struct Parser<'l> {
    lexer: &'l Lexer,
    // TODO Alias definitions, pending here-document contents, token to peek
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Line;
    use crate::source::Source;
    use std::num::NonZeroU64;
    use std::rc::Rc;

    #[test]
    fn display_for_error() {
        let number = NonZeroU64::new(1).unwrap();
        let line = Rc::new(Line {
            value: "".to_string(),
            number,
            source: Source::Unknown,
        });
        let location = Location {
            line,
            column: number,
        };
        let error = Error {
            cause: ErrorCause::EndOfInput,
            location,
        };
        assert_eq!(&format!("{}", error), "Incomplete command");
    }
}
