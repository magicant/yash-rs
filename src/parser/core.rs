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

use crate::source::lines;
use crate::source::Line;
use crate::source::Location;
use crate::source::Source;
use crate::source::SourceChar;
use std::fmt;
use std::future::ready;
use std::future::Future;
use std::num::NonZeroU64;
use std::pin::Pin;
use std::rc::Rc;

/// Types of errors that may happen in parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorCause {
    /// Uncategorized type of error.
    ///
    /// This error cause is used when the error type is so generic that no meaningful
    /// explanation can be provided.
    Unknown,
    /// End of input is reached while more characters are expected to be read.
    EndOfInput,
    // TODO Include the corresponding here-doc operator.
    /// A here-document operator is missing its corresponding content.
    MissingHereDocContent,
}

impl fmt::Display for ErrorCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCause::Unknown => f.write_str("Unknown error"),
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

/// Dummy trait for working around type error.
///
/// cf. https://stackoverflow.com/a/64325742
pub trait AsyncFnOnce<'a, T, R> {
    type Output: Future<Output = R>;
    fn call(self, t: &'a mut T) -> Self::Output;
}

impl<'a, T, F, R, Fut> AsyncFnOnce<'a, T, R> for F
where
    T: 'a,
    F: FnOnce(&'a mut T) -> Fut,
    Fut: Future<Output = R>,
{
    type Output = Fut;
    fn call(self, t: &'a mut T) -> Fut {
        self(t)
    }
}

/// Dummy trait for working around type error.
///
/// cf. https://stackoverflow.com/a/64325742
pub trait AsyncFnMut<'a, T, R> {
    type Output: Future<Output = R>;
    fn call(&mut self, t: &'a mut T) -> Self::Output;
}

impl<'a, T, F, R, Fut> AsyncFnMut<'a, T, R> for F
where
    T: 'a,
    F: FnMut(&'a mut T) -> Fut,
    Fut: Future<Output = R>,
{
    type Output = Fut;
    fn call(&mut self, t: &'a mut T) -> Fut {
        self(t)
    }
}

/// Lexical analyzer.
///
/// A lexer reads lines using an input function and parses the characters into tokens. It has an
/// internal buffer containing the characters that have been read and the position (or the
/// index) of the character that is to be parsed next.
///
/// `Lexer` has primitive functions such as [`peek`](Lexer::peek) and [`next`](Lexer::next) that
/// provide access to the character at the current position. Derived functions such as
/// [`skip_blanks_and_comment`](Lexer::skip_blanks_and_comment) depend on those primitives to
/// parse more complex structures in the source code.
pub struct Lexer {
    input: Box<dyn FnMut(&Context) -> Pin<Box<dyn Future<Output = Result<Line>>>>>,
    source: Vec<SourceChar>,
    index: usize,
    end_of_input: Option<Error>,
}

impl Lexer {
    /// Creates a new lexer with a fixed source code.
    #[must_use]
    pub fn with_source(source: Source, code: &str) -> Lexer {
        let lines = lines(source, code).map(Rc::new).collect::<Vec<_>>();
        let source = lines
            .iter()
            .map(Line::enumerate)
            .flatten()
            .collect::<Vec<_>>();
        let location = match source.last() {
            None => {
                let value = String::new();
                let one = NonZeroU64::new(1).unwrap();
                let source = Source::Unknown;
                let line = Rc::new(Line {
                    value,
                    number: one,
                    source,
                });
                Location { line, column: one }
            }
            Some(source_char) => {
                let mut location = source_char.location.clone();
                location.advance(1);
                location
            }
        };
        let error = Error {
            cause: ErrorCause::EndOfInput,
            location,
        };
        Lexer {
            input: Box::new(move |_| Box::pin(ready(Err(error.clone())))),
            source,
            index: 0,
            end_of_input: None,
        }
    }

    // TODO Probably we don't need this function
    /// Creates a new lexer with a fixed source code from unknown origin.
    ///
    /// This function is mainly for quick debugging purpose. Using in productions is not
    /// recommended because it does not provide meaningful [`Source`] on error.
    #[must_use]
    pub fn with_unknown_source(code: &str) -> Lexer {
        Lexer::with_source(Source::Unknown, code)
    }

    /// Peeks the next character.
    ///
    /// Returns [`EndOfInput`](ErrorCause::EndOfInput) if reached the end of input.
    #[must_use]
    pub async fn peek(&mut self) -> Result<SourceChar> {
        loop {
            if let Some(c) = self.source.get(self.index) {
                return Ok(c.clone());
            }

            if let Some(ref e) = self.end_of_input {
                assert_eq!(self.index, self.source.len());
                return Err(e.clone());
            }

            // Read more input
            match (self.input)(&Context).await {
                Ok(line) => self.source.extend(Rc::new(line).enumerate()),
                Err(e) => {
                    self.end_of_input = Some(e.clone());
                    return Err(e);
                }
            }
        }
    }

    /// Peeks the next character and, if the given decider function returns true for it, advances
    /// the position.
    ///
    /// Returns the consumed character if the function returned true. Returns an
    /// [`Unknown`](ErrorCause::Unknown) error if the function returned false. Returns the error
    /// intact if the input function returned an error, including the end-of-input case.
    pub async fn next_if<F>(&mut self, f: F) -> Result<SourceChar>
    where
        F: FnOnce(char) -> bool,
    {
        let c = self.peek().await?;
        if f(c.value) {
            self.index += 1;
            Ok(c)
        } else {
            Err(Error {
                cause: ErrorCause::Unknown,
                location: c.location,
            })
        }
    }

    /// Reads the next character, advancing the position.
    ///
    /// Returns [`EndOfInput`](ErrorCause::EndOfInput) if reached the end of input.
    pub async fn next(&mut self) -> Result<SourceChar> {
        let r = self.peek().await;
        if r.is_ok() {
            self.index += 1;
        }
        r
    }

    /// Applies the given parser and updates the current position only if the parser succeeds.
    ///
    /// This function can be used to cancel the effect of failed parsing so that the consumed
    /// characters can be parsed by another parser. Note that `maybe` only rewinds the position. It
    /// does not undo the effect on the buffer containing the characters read while parsing.
    pub async fn maybe<F, R>(&mut self, f: F) -> Result<R>
    where
        F: for<'a> AsyncFnOnce<'a, Lexer, Result<R>>,
    {
        let old_index = self.index;
        let r = f.call(self).await;
        if r.is_err() {
            self.index = old_index;
        }
        r
    }

    /// Applies the given parser repeatedly until it fails.
    ///
    /// This function implicitly applies [`Lexer::maybe`] so that the position is left just after the last
    /// successful parse.
    ///
    /// Returns a vector of the successful results and the error that stopped the repetition.
    pub async fn many<F, R>(&mut self, mut f: F) -> (Vec<R>, Error)
    where
        F: for<'a> AsyncFnMut<'a, Lexer, Result<R>>,
    {
        let mut results = vec![];
        loop {
            let old_index = self.index;
            match f.call(self).await {
                Ok(r) => results.push(r),
                Err(e) => {
                    self.index = old_index;
                    break (results, e);
                }
            }
        }
    }
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
    /// Lexer that provides tokens.
    lexer: &'l mut Lexer,

    /// Token to parse next.
    ///
    /// This value is an option of a result. It is `None` when the next token is not yet parsed by
    /// the lexer. It is `Some(Err(_))` if the lexer has failed.
    token: Option<Result<crate::syntax::Word>>,
    // TODO Alias definitions, pending here-document contents
}

impl Parser<'_> {
    /// Creates a new parser based on the given lexer.
    pub fn new(lexer: &mut Lexer) -> Parser {
        Parser { lexer, token: None }
    }

    // TODO Replace Word in the return type with Token
    /// Reads a next token if the current token is `None`.
    async fn require_token(&mut self) {
        if self.token.is_none() {
            self.lexer.skip_blanks_and_comment().await;
            let result = self.lexer.word().await;
            self.token = Some(if let Ok(word) = result {
                if word.units.is_empty() {
                    Err(Error {
                        cause: ErrorCause::EndOfInput,
                        location: word.location,
                    })
                } else {
                    Ok(word)
                }
            } else {
                result
            });
        }
    }

    // TODO Replace Word in the return type with Token
    /// Returns a reference to the current token.
    ///
    /// If the current token is not yet read from the underlying lexer, it is read.
    pub async fn peek_token(&mut self) -> &Result<crate::syntax::Word> {
        self.require_token().await;
        self.token.as_ref().unwrap()
    }

    // TODO Replace Word in the return type with Token
    /// Consumes the current token.
    pub async fn take_token(&mut self) -> Result<crate::syntax::Word> {
        self.require_token().await;
        self.token.take().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
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
        assert_eq!(format!("{}", error), "Incomplete command");
    }

    #[test]
    fn lexer_with_empty_source() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let e = block_on(lexer.peek()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::EndOfInput);
        assert_eq!(e.location.line.value, "");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }

    #[test]
    fn lexer_with_multiline_source() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo\nbar\n");

        let c = block_on(lexer.peek()).unwrap();
        assert_eq!(c.value, 'f');
        assert_eq!(c.location.line.value, "foo\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 1);

        let c2 = block_on(lexer.peek()).unwrap();
        assert_eq!(c, c2);
        let c2 = block_on(lexer.peek()).unwrap();
        assert_eq!(c, c2);
        let c2 = block_on(lexer.next()).unwrap();
        assert_eq!(c, c2);

        let c = block_on(lexer.next()).unwrap();
        assert_eq!(c.value, 'o');
        assert_eq!(c.location.line.value, "foo\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 2);

        let c = block_on(lexer.next()).unwrap();
        assert_eq!(c.value, 'o');
        assert_eq!(c.location.line.value, "foo\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 3);

        let c = block_on(lexer.next()).unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.line.value, "foo\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 4);

        let c = block_on(lexer.next()).unwrap();
        assert_eq!(c.value, 'b');
        assert_eq!(c.location.line.value, "bar\n");
        assert_eq!(c.location.line.number.get(), 2);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 1);

        let c = block_on(lexer.next()).unwrap();
        assert_eq!(c.value, 'a');
        assert_eq!(c.location.line.value, "bar\n");
        assert_eq!(c.location.line.number.get(), 2);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 2);

        let c = block_on(lexer.next()).unwrap();
        assert_eq!(c.value, 'r');
        assert_eq!(c.location.line.value, "bar\n");
        assert_eq!(c.location.line.number.get(), 2);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 3);

        let c = block_on(lexer.next()).unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.line.value, "bar\n");
        assert_eq!(c.location.line.number.get(), 2);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 4);

        let e = block_on(lexer.peek()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::EndOfInput);
        assert_eq!(e.location.line.value, "bar\n");
        assert_eq!(e.location.line.number.get(), 2);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 5);

        let e2 = block_on(lexer.peek()).unwrap_err();
        assert_eq!(e, e2);
        let e2 = block_on(lexer.next()).unwrap_err();
        assert_eq!(e, e2);
        let e2 = block_on(lexer.peek()).unwrap_err();
        assert_eq!(e, e2);
    }

    #[test]
    fn lexer_next_if() {
        let mut lexer = Lexer::with_source(Source::Unknown, "word\n");

        let mut called = 0;
        let c = block_on(lexer.next_if(|c| {
            assert_eq!(c, 'w');
            called += 1;
            true
        }))
        .unwrap();
        assert_eq!(called, 1);
        assert_eq!(c.value, 'w');
        assert_eq!(c.location.line.value, "word\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 1);

        let mut called = 0;
        let e = block_on(lexer.next_if(|c| {
            assert_eq!(c, 'o');
            called += 1;
            false
        }))
        .unwrap_err();
        assert_eq!(called, 1);
        assert_eq!(e.cause, ErrorCause::Unknown);
        assert_eq!(e.location.line.value, "word\n");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 2);

        let mut called = 0;
        let e = block_on(lexer.next_if(|c| {
            assert_eq!(c, 'o');
            called += 1;
            false
        }))
        .unwrap_err();
        assert_eq!(called, 1);
        assert_eq!(e.cause, ErrorCause::Unknown);
        assert_eq!(e.location.line.value, "word\n");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 2);

        let mut called = 0;
        let c = block_on(lexer.next_if(|c| {
            assert_eq!(c, 'o');
            called += 1;
            true
        }))
        .unwrap();
        assert_eq!(called, 1);
        assert_eq!(c.value, 'o');
        assert_eq!(c.location.line.value, "word\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 2);
    }

    #[test]
    fn lexer_maybe_success() {
        let mut lexer = Lexer::with_source(Source::Unknown, "abc");

        async fn f(l: &mut Lexer) -> Result<SourceChar> {
            l.next().await?;
            l.next().await
        }
        let x = lexer.maybe(f);
        let c = block_on(x).unwrap();
        assert_eq!(c.value, 'b');

        let c = block_on(lexer.next()).unwrap();
        assert_eq!(c.value, 'c');
    }

    #[test]
    fn lexer_maybe_failure() {
        let mut lexer = Lexer::with_source(Source::Unknown, "abc");

        async fn f(l: &mut Lexer) -> Result<SourceChar> {
            l.next().await?;
            let SourceChar { location, .. } = l.next().await.unwrap();
            let cause = ErrorCause::EndOfInput;
            Err(Error { cause, location })
        }
        let x = lexer.maybe(f);
        let Error { cause, location } = block_on(x).unwrap_err();
        assert_eq!(cause, ErrorCause::EndOfInput);
        assert_eq!(location.column.get(), 2);

        let c = block_on(lexer.next()).unwrap();
        assert_eq!(c.value, 'a');
        assert_eq!(c.location.column.get(), 1);
    }

    #[test]
    fn lexer_many_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");

        async fn f(l: &mut Lexer) -> Result<SourceChar> {
            l.next_if(|c| c == 'a').await?;
            l.next_if(|c| c == 'b').await
        }
        let (r, e) = block_on(lexer.many(f));
        assert!(r.is_empty());
        assert_eq!(e.cause, ErrorCause::EndOfInput);
        assert_eq!(e.location.line.value, "");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }

    #[test]
    fn lexer_many_one() {
        let mut lexer = Lexer::with_source(Source::Unknown, "ab");

        async fn f(l: &mut Lexer) -> Result<SourceChar> {
            l.next_if(|c| c == 'a').await?;
            l.next_if(|c| c == 'b').await
        }
        let (r, e) = block_on(lexer.many(f));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].value, 'b');
        assert_eq!(e.cause, ErrorCause::EndOfInput);
        assert_eq!(e.location.line.value, "ab");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 3);
    }
    #[test]
    fn lexer_many_three() {
        let mut lexer = Lexer::with_source(Source::Unknown, "xyxyxyxz");

        async fn f(l: &mut Lexer) -> Result<SourceChar> {
            l.next_if(|c| c == 'x').await?;
            l.next_if(|c| c == 'y').await
        }
        let (r, e) = block_on(lexer.many(f));
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].value, 'y');
        assert_eq!(r[1].value, 'y');
        assert_eq!(r[2].value, 'y');
        assert_eq!(e.cause, ErrorCause::Unknown);
        assert_eq!(e.location.line.value, "xyxyxyxz");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 8);
    }
}
