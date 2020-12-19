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

//! Lexical analyzer.
//!
//! TODO Elaborate

mod core {

    use crate::parser::core::AsyncFnMut;
    use crate::parser::core::AsyncFnOnce;
    use crate::parser::core::Error;
    use crate::parser::core::ErrorCause;
    use crate::parser::core::Result;
    use crate::source::lines;
    use crate::source::Line;
    use crate::source::Location;
    use crate::source::Source;
    use crate::source::SourceChar;
    use crate::syntax::Word;
    use std::fmt;
    use std::future::ready;
    use std::future::Future;
    use std::num::NonZeroU64;
    use std::pin::Pin;
    use std::rc::Rc;

    // TODO Move to crate::input
    /// Current state in which input is read.
    ///
    /// The context is passed to the input function so that it can read the input in a
    /// context-dependent way.
    ///
    /// Currently, this structure is empty. It may be extended to provide with some useful data in
    /// future versions.
    #[derive(Debug)]
    pub struct Context;

    /// Token identifier, or classification of tokens.
    ///
    /// This enum classifies a token as defined in POSIX XCU 2.10.1 Shell Grammar Lexical
    /// Conventions, but does not reflect further distinction defined in POSIX XCU 2.10.2 Shell
    /// Grammar Rules.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum TokenId {
        /// `TOKEN`
        Token,
        // TODO other token identifiers: operators and IO_NUMBER
    }

    /// Result of lexical analysis produced by the [`Lexer`].
    #[derive(Debug)]
    pub struct Token {
        /// Content of the token.
        ///
        /// The word value always contains at least one [unit](crate::syntax::WordUnit), regardless
        /// of whether the token is an operator.
        pub word: Word,
        /// Token identifier.
        pub id: TokenId,
    }

    impl fmt::Display for Token {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.word)
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
        // TODO The input function type should be defined in crate::input
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
}

pub use self::core::*;
use crate::parser::core::Error;
use crate::parser::core::ErrorCause;
use crate::parser::core::Result;
use crate::source::SourceChar;
use crate::syntax::*;

impl Lexer {
    /// Skips a character if the given function returns true for it.
    pub async fn skip_if<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(char) -> bool,
    {
        self.next_if(f).await.is_ok()
    }

    /// Skips a line continuation, if any.
    ///
    /// If there is a line continuation at the current position, this function skips it and returns
    /// `Ok(())`. Otherwise, it returns an [`Unknown`](ErrorCause::Unknown) error without consuming
    /// any characters.
    pub async fn maybe_line_continuation(&mut self) -> Result<()> {
        async fn line_continuation(this: &mut Lexer) -> Result<()> {
            this.next_if(|c| c == '\\').await?;
            this.next_if(|c| c == '\n').await?;
            Ok(())
        }
        self.maybe(line_continuation).await
    }
    // TODO Change maybe_line_continuation to line_continuation or remove implicit `maybe` effect
    // from `many`, as current `many(maybe_line_continuation)` doubles the `maybe` effect
    // redundantly.

    /// Skips blank characters until reaching a non-blank.
    ///
    /// This function also skips line continuations.
    pub async fn skip_blanks(&mut self) {
        // TODO Support locale-dependent decision
        loop {
            let _ = self.many(Lexer::maybe_line_continuation).await;
            if !self.skip_if(|c| c != '\n' && c.is_whitespace()).await {
                break;
            }
        }
    }

    /// Skips a comment, if any.
    ///
    /// A comment ends just before a newline. The newline is *not* part of the comment.
    ///
    /// This function does not recognize any line continuations.
    pub async fn skip_comment(&mut self) {
        if !self.skip_if(|c| c == '#').await {
            return;
        }

        while self.skip_if(|c| c != '\n').await {}
    }

    /// Skips blank characters and a comment, if any.
    ///
    /// This function also skips line continuations between blanks. It is the same as
    /// [`skip_blanks`](Lexer::skip_blanks) followed by [`skip_comment`](Lexer::skip_comment).
    pub async fn skip_blanks_and_comment(&mut self) {
        self.skip_blanks().await;
        self.skip_comment().await;
    }

    // TODO Need more parameters to control how the word should be parsed. Especially:
    //  * What delimiter ends the word?
    //  * Allow tilde expansion?
    /// Parses a word token.
    pub async fn word(&mut self) -> Result<Word> {
        let SourceChar { location, .. } = self.peek().await?;

        let mut units = vec![];
        loop {
            // TODO Parse the word correctly
            match self.next_if(|c| c != '\n' && !c.is_whitespace()).await {
                Ok(sc) => units.push(Unquoted(Literal(sc.value))),
                Err(Error { cause, .. }) if cause == ErrorCause::Unknown => break,
                Err(Error { cause, .. })
                    if cause == ErrorCause::EndOfInput && !units.is_empty() =>
                {
                    break
                }
                Err(e) => return Err(e),
            }
        }
        Ok(Word { units, location })
    }

    /// Parses a token.
    ///
    /// A successfully parsed token's word cannot be empty.
    pub async fn token(&mut self) -> Result<Token> {
        // TODO parse operators and IO_NUMBER
        let word = self.word().await?;
        if word.units.is_empty() {
            Err(Error{cause: ErrorCause::EndOfInput, location: word.location})
        } else {
            Ok(Token { word, id: TokenId::Token })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use futures::executor::block_on;

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

    #[test]
    fn lexer_maybe_line_continuation_success() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\\n");

        assert!(block_on(lexer.maybe_line_continuation()).is_ok());

        let e = block_on(lexer.peek()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::EndOfInput);
        assert_eq!(e.location.line.value, "\\\n");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 3);
    }

    #[test]
    fn lexer_maybe_line_continuation_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");

        let e = block_on(lexer.maybe_line_continuation()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::EndOfInput);
        assert_eq!(e.location.line.value, "");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);

        let e = block_on(lexer.peek()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::EndOfInput);
        assert_eq!(e.location.column.get(), 1);
    }

    #[test]
    fn lexer_maybe_line_continuation_not_backslash() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\n");

        let e = block_on(lexer.maybe_line_continuation()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Unknown);
        assert_eq!(e.location.line.value, "\n");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);

        let c = block_on(lexer.peek()).unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.column.get(), 1);
    }

    #[test]
    fn lexer_maybe_line_continuation_only_backslash() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\");

        let e = block_on(lexer.maybe_line_continuation()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::EndOfInput);
        assert_eq!(e.location.line.value, "\\");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 2);

        let c = block_on(lexer.peek()).unwrap();
        assert_eq!(c.value, '\\');
        assert_eq!(c.location.column.get(), 1);
    }

    #[test]
    fn lexer_maybe_line_continuation_not_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\\\");

        let e = block_on(lexer.maybe_line_continuation()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Unknown);
        assert_eq!(e.location.line.value, "\\\\");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 2);

        let c = block_on(lexer.peek()).unwrap();
        assert_eq!(c.value, '\\');
        assert_eq!(c.location.column.get(), 1);
    }

    #[test]
    fn lexer_skip_blanks() {
        let mut lexer = Lexer::with_source(Source::Unknown, " \t w");

        let c = block_on(async {
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
        let c = block_on(async {
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
        let mut lexer = Lexer::with_source(Source::Unknown, "\n");
        let (c1, c2) = block_on(async {
            let c1 = lexer.peek().await;
            lexer.skip_blanks().await;
            let c2 = lexer.peek().await;
            (c1, c2)
        });
        assert_eq!(c1, c2);
    }

    #[test]
    fn lexer_skip_blanks_skips_line_continuations() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\\n  \\\n\\\n\\\n \\\nX");
        let c = block_on(async {
            lexer.skip_blanks().await;
            lexer.peek().await
        })
        .unwrap();
        assert_eq!(c.value, 'X');
        assert_eq!(c.location.line.value, "X");
        assert_eq!(c.location.line.number.get(), 6);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 1);

        let mut lexer = Lexer::with_source(Source::Unknown, "  \\\n\\\n  \\\n Y");
        let c = block_on(async {
            lexer.skip_blanks().await;
            lexer.peek().await
        })
        .unwrap();
        assert_eq!(c.value, 'Y');
        assert_eq!(c.location.line.value, " Y");
        assert_eq!(c.location.line.number.get(), 4);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 2);
    }

    #[test]
    fn lexer_skip_comment_no_comment() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\n");
        let (c1, c2) = block_on(async {
            let c1 = lexer.peek().await;
            lexer.skip_comment().await;
            let c2 = lexer.peek().await;
            (c1, c2)
        });
        assert_eq!(c1, c2);
    }

    #[test]
    fn lexer_skip_comment_empty_comment() {
        let mut lexer = Lexer::with_source(Source::Unknown, "#\n");

        let c = block_on(async {
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
        let c = block_on(async {
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
        let mut lexer = Lexer::with_source(Source::Unknown, "### foo bar\\\n");

        let c = block_on(async {
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
        let c = block_on(async {
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
        let mut lexer = Lexer::with_source(Source::Unknown, "#comment");

        let e = block_on(async {
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
        let e = block_on(async {
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

    #[test]
    fn lexer_token_empty() {
        // If there's no word unit that can be parsed, it is the end of input.
        let mut lexer = Lexer::with_source(Source::Unknown, "");

        let e = block_on(lexer.token()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::EndOfInput);
        assert_eq!(e.location.line.value, "");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }

    #[test]
    fn lexer_token_non_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "abc ");

        let t = block_on(lexer.token()).unwrap();
        assert_eq!(t.word.units.len(), 3);
        assert_eq!(t.word.units[0], WordUnit::Unquoted(DoubleQuotable::Literal('a')));
        assert_eq!(t.word.units[1], WordUnit::Unquoted(DoubleQuotable::Literal('b')));
        assert_eq!(t.word.units[2], WordUnit::Unquoted(DoubleQuotable::Literal('c')));
        assert_eq!(t.word.location.line.value, "abc ");
        assert_eq!(t.word.location.line.number.get(), 1);
        assert_eq!(t.word.location.line.source, Source::Unknown);
        assert_eq!(t.word.location.column.get(), 1);
        assert_eq!(t.id, TokenId::Token);
    }
}
