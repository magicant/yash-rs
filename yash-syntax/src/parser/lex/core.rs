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

//! Fundamental building blocks for the lexical analyzer.

use super::keyword::Keyword;
pub use super::op::Operator;
use crate::alias::Alias;
use crate::input::Context;
use crate::input::Input;
use crate::input::Memory;
use crate::parser::core::AsyncFnMut;
use crate::parser::core::Error;
use crate::parser::core::Result;
use crate::source::lines;
use crate::source::Location;
use crate::source::Source;
use crate::source::SourceChar;
use crate::syntax::Word;
use std::fmt;
use std::future::Future;
use std::num::NonZeroU64;
use std::pin::Pin;
use std::rc::Rc;

/// Returns true if the character is a blank character.
pub fn is_blank(c: char) -> bool {
    // TODO locale
    c != '\n' && c.is_whitespace()
}

/// Result of [`Lexer::peek_char_or_end`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PeekChar<'a> {
    Char(&'a SourceChar),
    EndOfInput(&'a Location),
}

impl<'a> PeekChar<'a> {
    /// Converts `PeekChar` to `Option`.
    #[must_use]
    fn as_option<'b>(self: &'b PeekChar<'a>) -> Option<&'a SourceChar> {
        match self {
            PeekChar::Char(c) => Some(c),
            PeekChar::EndOfInput(_) => None,
        }
    }

    /// Returns the location that was peeked.
    #[must_use]
    fn location<'b>(self: &'b PeekChar<'a>) -> &'a Location {
        match self {
            PeekChar::Char(c) => &c.location,
            PeekChar::EndOfInput(l) => l,
        }
    }
}

/// Token identifier, or classification of tokens.
///
/// This enum classifies a token as defined in POSIX XCU 2.10.1 Shell Grammar Lexical
/// Conventions, but does not exactly reflect further distinction defined in
/// POSIX XCU 2.10.2 Shell Grammar Rules.
///
/// For convenience, the special token identifier `EndOfInput` is included.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenId {
    /// `TOKEN`
    ///
    /// If this token _looks like_ a reserved word, this variant has some
    /// associated `Keyword` that describes the word. However, it depends on
    /// context whether a token is actually regarded as a reserved word or
    /// just as an ordinary word. You must ensure that you're in an
    /// applicable context when examining the `Keyword` value.
    Token(Option<Keyword>),
    /// Operator
    Operator(Operator),
    /// `IO_NUMBER`
    IoNumber,
    /// Imaginary token identifier for the end of input.
    EndOfInput,
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
    /// Position of the first character of the word.
    pub index: usize,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.word)
    }
}

/// State of the input function in a lexer.
#[derive(Clone, Debug)]
enum InputState {
    Alive,
    EndOfInput(Location),
    Error(Error),
}

/// Lexical analyzer.
///
/// A lexer reads lines using an input function and parses the characters into tokens. It has an
/// internal buffer containing the characters that have been read and the position (or the
/// index) of the character that is to be parsed next.
///
/// `Lexer` has primitive functions such as [`peek_char`](Lexer::peek_char) that provide access
/// to the character at the current position. Derived functions such as
/// [`skip_blanks_and_comment`](Lexer::skip_blanks_and_comment) depend on those primitives to
/// parse more complex structures in the source code.
pub struct Lexer {
    input: Box<dyn Input>,
    state: InputState,
    source: Vec<SourceChar>,
    index: usize,
}

impl Lexer {
    /// Creates a new lexer that reads using the given input function.
    #[must_use]
    pub fn new(input: Box<dyn Input>) -> Lexer {
        Lexer {
            input,
            state: InputState::Alive,
            source: Vec::new(),
            index: 0,
        }
    }

    /// Creates a new lexer with a fixed source code.
    #[must_use]
    pub fn with_source(source: Source, code: &str) -> Lexer {
        Lexer::new(Box::new(Memory::new(source, code)))
    }

    /// Peeks the next character.
    async fn peek_char_or_end(&mut self) -> Result<PeekChar<'_>> {
        loop {
            if self.index < self.source.len() {
                return Ok(PeekChar::Char(&self.source[self.index]));
            }

            match self.state {
                InputState::Alive => (),
                InputState::EndOfInput(ref location) => return Ok(PeekChar::EndOfInput(location)),
                InputState::Error(ref error) => return Err(error.clone()),
            }

            // Read more input
            match self.input.next_line(&Context).await {
                Ok(line) => {
                    if line.value.is_empty() {
                        // End of input
                        let location = if let Some(c) = self.source.last() {
                            // TODO correctly count line number after newline
                            //if sc.value == '\n' {
                            //} else {
                            let mut location = c.location.clone();
                            location.advance(1);
                            location
                        //}
                        } else {
                            // Completely empty source
                            Location {
                                line: Rc::new(line),
                                column: NonZeroU64::new(1).unwrap(),
                            }
                        };
                        self.state = InputState::EndOfInput(location);
                    } else {
                        // Successful read
                        self.source.extend(Rc::new(line).enumerate())
                    }
                }
                Err((location, io_error)) => {
                    self.state = InputState::Error(Error {
                        cause: io_error.into(),
                        location,
                    });
                }
            }
        }
    }

    /// Peeks the next character.
    ///
    /// If the end of input is reached, `Ok(None)` is returned. On error, `Err(_)` is returned.
    pub async fn peek_char(&mut self) -> Result<Option<&SourceChar>> {
        self.peek_char_or_end().await.map(|p| p.as_option())
    }

    /// Returns the location of the next character.
    ///
    /// If there is no more character (that is, it is the end of input), an imaginary location
    /// is returned that would be returned if a character existed.
    ///
    /// This function required a mutable reference to `self` since it may need to read a next
    /// line if it is not yet read.
    pub async fn location(&mut self) -> Result<&Location> {
        self.peek_char_or_end().await.map(|p| p.location())
    }

    /// Consumes the next character.
    ///
    /// This function must be called after [`peek_char`](Lexer::peek_char) has successfully
    /// returned the character. Consuming a character that has not yet been peeked would result
    /// in a panic!
    pub fn consume_char(&mut self) {
        assert!(
            self.index < self.source.len(),
            "A character must have been peeked before being consumed: index={}",
            self.index
        );
        self.index += 1;
    }

    /// Returns the position of the next character, counted from zero.
    ///
    /// ```
    /// # use yash_syntax::parser::lex::Lexer;
    /// # use yash_syntax::source::Source;
    /// futures::executor::block_on(async {
    ///     let mut lexer = Lexer::with_source(Source::Unknown, "abc");
    ///     assert_eq!(lexer.index(), 0);
    ///     let _ = lexer.peek_char().await;
    ///     assert_eq!(lexer.index(), 0);
    ///     lexer.consume_char();
    ///     assert_eq!(lexer.index(), 1);
    /// })
    /// ```
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }

    /// Moves the current position back to the given index so that characters that have been
    /// consumed can be read again.
    ///
    /// The given index must not be larger than the [current index](Lexer::index), or this
    /// function would panic.
    ///
    /// ```
    /// # use yash_syntax::parser::lex::Lexer;
    /// # use yash_syntax::source::Source;
    /// futures::executor::block_on(async {
    ///     let mut lexer = Lexer::with_source(Source::Unknown, "abc");
    ///     let saved_index = lexer.index();
    ///     let a = lexer.peek_char().await.unwrap().cloned();
    ///     lexer.consume_char();
    ///     let b = lexer.peek_char().await.unwrap().cloned();
    ///     lexer.rewind(saved_index);
    ///     let a2 = lexer.peek_char().await.unwrap().cloned();
    ///     assert_eq!(a, a2);
    ///     assert_ne!(a, b);
    /// })
    /// ```
    pub fn rewind(&mut self, index: usize) {
        assert!(
            index <= self.index,
            "The new index {} must not be larger than the current index {}",
            index,
            self.index
        );
        self.index = index;
    }

    /// Peeks the next character and, if the given decider function returns true for it,
    /// advances the position.
    ///
    /// Returns the consumed character if the function returned true. Returns `Ok(None)` if it
    /// returned false or there is no more character.
    pub async fn consume_char_if<F>(&mut self, f: F) -> Result<Option<&SourceChar>>
    where
        F: FnOnce(char) -> bool,
    {
        match self.peek_char().await? {
            Some(c) if f(c.value) => {
                let index = self.index;
                self.consume_char();
                Ok(Some(&self.source[index]))
            }
            _ => Ok(None),
        }
    }

    /// Applies the given parser repeatedly until it fails.
    ///
    /// Returns a vector of accumulated successful results from the parser.
    ///
    /// A parse result is considered successful if it is an `Ok(Some(_))`, failed if
    /// `Ok(None)` or `Err(_)`. In case of `Err(_)`, all the accumulated results are abandoned
    /// and only the error is returned.
    ///
    /// When the parser fails, the current position is reset to the position just after the
    /// last successful parse. This cancels the effect of the failed parse that may have
    /// consumed some characters.
    pub async fn many<F, R>(&mut self, mut f: F) -> Result<Vec<R>>
    where
        F: for<'a> AsyncFnMut<'a, Lexer, Result<Option<R>>>,
    {
        let mut results = vec![];
        loop {
            let old_index = self.index;
            match f.call(self).await? {
                Some(r) => results.push(r),
                None => {
                    self.index = old_index;
                    return Ok(results);
                }
            }
        }
    }

    /// Performs alias substitution right before the current position.
    ///
    /// This function must be called just after a [word](Lexer::word) has been parsed that
    /// matches the name of the argument alias. No check is done in this function that there is
    /// a matching word before the current position. The characters starting from the `begin`
    /// index up to the current position are silently replaced with the alias value.
    ///
    /// The resulting part of code will be characters with a [`Source::Alias`] origin.
    ///
    /// After the substitution, the position will be set before the replaced string.
    ///
    /// # Panics
    ///
    /// If the replaced part is empty, i.e., `begin >= self.index()`.
    pub fn substitute_alias(&mut self, begin: usize, alias: &Rc<Alias>) {
        let end = self.index;
        if begin >= end {
            panic!("Lexer::substitute_alias: begin={}, end={}", begin, end);
        }

        let original = self.source[begin].location.clone();
        let source = Source::Alias {
            original,
            alias: alias.clone(),
        };
        let mut repl = vec![];
        for line in lines(source, &alias.replacement) {
            repl.extend(Rc::new(line).enumerate());
        }

        self.source.splice(begin..end, repl);
        self.index = begin;
    }

    /// Tests if the given index is after the replacement string of alias
    /// substitution that ends with a blank.
    ///
    /// # Panics
    ///
    /// If `index` is larger than the currently read index.
    pub fn is_after_blank_ending_alias(&self, index: usize) -> bool {
        fn ends_with_blank(s: &str) -> bool {
            s.chars().rev().next().map_or(false, is_blank)
        }
        fn is_same_alias(alias: &Alias, sc: Option<&SourceChar>) -> bool {
            match sc {
                None => false,
                Some(sc) => sc.location.line.source.is_alias_for(&alias.name),
            }
        }

        for index in (0..index).rev() {
            let sc = &self.source[index];

            if !is_blank(sc.value) {
                return false;
            }

            if let Source::Alias { ref alias, .. } = sc.location.line.source {
                #[allow(clippy::collapsible_if)]
                if ends_with_blank(&alias.replacement) {
                    if !is_same_alias(alias, self.source.get(index + 1)) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Parses an optional compound list that is the content of a command
    /// substitution.
    ///
    /// This function consumes characters until a token that cannot be the
    /// beginning of an and-or list is found and returns the string that was
    /// consumed.
    pub async fn inner_program(&mut self) -> Result<String> {
        let begin = self.index;

        let mut parser = super::super::Parser::new(self);
        parser.maybe_compound_list().await?;

        let end = parser.peek_token().await?.index;
        self.rewind(end);
        Ok(self.source[begin..end].iter().map(|c| c.value).collect())
    }

    /// Like [`Lexer::inner_program`], but returns the future in a pinned box.
    pub fn inner_program_boxed(&mut self) -> Pin<Box<dyn Future<Output = Result<String>> + '_>> {
        Box::pin(self.inner_program())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::core::ErrorCause;
    use crate::parser::core::SyntaxError;
    use futures::executor::block_on;
    use std::future::ready;

    #[test]
    fn lexer_with_empty_source() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_with_multiline_source() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo\nbar\n");

        let c = block_on(lexer.peek_char()).unwrap().unwrap().clone();
        assert_eq!(c.value, 'f');
        assert_eq!(c.location.line.value, "foo\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 1);

        let c2 = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c, *c2);
        let c2 = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c, *c2);
        let c2 = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c, *c2);
        lexer.consume_char();

        let c = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c.value, 'o');
        assert_eq!(c.location.line.value, "foo\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 2);
        lexer.consume_char();

        let c = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c.value, 'o');
        assert_eq!(c.location.line.value, "foo\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 3);
        lexer.consume_char();

        let c = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.line.value, "foo\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 4);
        lexer.consume_char();

        let c = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c.value, 'b');
        assert_eq!(c.location.line.value, "bar\n");
        assert_eq!(c.location.line.number.get(), 2);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 1);
        lexer.consume_char();

        let c = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c.value, 'a');
        assert_eq!(c.location.line.value, "bar\n");
        assert_eq!(c.location.line.number.get(), 2);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 2);
        lexer.consume_char();

        let c = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c.value, 'r');
        assert_eq!(c.location.line.value, "bar\n");
        assert_eq!(c.location.line.number.get(), 2);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 3);
        lexer.consume_char();

        let c = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.line.value, "bar\n");
        assert_eq!(c.location.line.number.get(), 2);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 4);
        lexer.consume_char();

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_peek_char_io_error() {
        #[derive(Debug)]
        struct Failing;
        impl fmt::Display for Failing {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Failing")
            }
        }
        impl std::error::Error for Failing {}
        impl Input for Failing {
            fn next_line(
                &mut self,
                _: &Context,
            ) -> Pin<Box<dyn Future<Output = crate::input::Result>>> {
                let location = Location::dummy("line".to_string());
                let error = std::io::Error::new(std::io::ErrorKind::Other, Failing);
                Box::pin(ready(Err((location, error))))
            }
        }
        let mut lexer = Lexer::new(Box::new(Failing));

        let e = block_on(lexer.peek_char()).unwrap_err();
        if let ErrorCause::Io(io_error) = e.cause {
            assert_eq!(io_error.kind(), std::io::ErrorKind::Other);
        } else {
            panic!("expected IoError, but actually {}", e.cause)
        }
        assert_eq!(e.location.line.value, "line");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }

    #[test]
    fn lexer_consume_char() {
        let mut lexer = Lexer::with_source(Source::Unknown, "abc");

        block_on(async {
            let _ = lexer.peek_char().await;
            lexer.consume_char();

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, 'b');
            assert_eq!(c.location.line.value, "abc");
            assert_eq!(c.location.line.number.get(), 1);
            assert_eq!(c.location.line.source, Source::Unknown);
            assert_eq!(c.location.column.get(), 2);
            lexer.consume_char();

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, 'c');
            assert_eq!(c.location.line.value, "abc");
            assert_eq!(c.location.line.number.get(), 1);
            assert_eq!(c.location.line.source, Source::Unknown);
            assert_eq!(c.location.column.get(), 3);
        });
    }

    #[test]
    #[should_panic(expected = "A character must have been peeked before being consumed: index=0")]
    fn lexer_consume_char_without_peeking() {
        let mut lexer = Lexer::with_source(Source::Unknown, "abc");
        lexer.consume_char();
    }

    #[test]
    fn lexer_index() {
        let mut lexer = Lexer::with_source(Source::Unknown, "abc");

        block_on(async {
            assert_eq!(lexer.index(), 0);

            let _ = lexer.peek_char().await;
            assert_eq!(lexer.index(), 0);
            lexer.consume_char();
            assert_eq!(lexer.index(), 1);

            let _ = lexer.peek_char().await;
            assert_eq!(lexer.index(), 1);
            lexer.consume_char();
            assert_eq!(lexer.index(), 2);

            let _ = lexer.peek_char().await;
            assert_eq!(lexer.index(), 2);
            lexer.consume_char();
            assert_eq!(lexer.index(), 3);
        });
    }

    #[test]
    fn lexer_rewind() {
        let mut lexer = Lexer::with_source(Source::Unknown, "abc");
        lexer.rewind(0);

        block_on(async {
            let _ = lexer.peek_char().await;
            lexer.consume_char();
            let _ = lexer.peek_char().await;
            lexer.consume_char();
            lexer.rewind(0);

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, 'a');
            assert_eq!(c.location.line.value, "abc");
            assert_eq!(c.location.line.number.get(), 1);
            assert_eq!(c.location.line.source, Source::Unknown);
            assert_eq!(c.location.column.get(), 1);
        });
    }

    #[test]
    #[should_panic(expected = "The new index 1 must not be larger than the current index 0")]
    fn lexer_rewind_invalid_index() {
        let mut lexer = Lexer::with_source(Source::Unknown, "abc");
        lexer.rewind(1);
    }

    #[test]
    fn lexer_consume_char_if() {
        let mut lexer = Lexer::with_source(Source::Unknown, "word\n");

        let mut called = 0;
        let c = block_on(lexer.consume_char_if(|c| {
            assert_eq!(c, 'w');
            called += 1;
            true
        }))
        .unwrap()
        .unwrap();
        assert_eq!(called, 1);
        assert_eq!(c.value, 'w');
        assert_eq!(c.location.line.value, "word\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 1);

        let mut called = 0;
        let r = block_on(lexer.consume_char_if(|c| {
            assert_eq!(c, 'o');
            called += 1;
            false
        }));
        assert_eq!(called, 1);
        assert_eq!(r, Ok(None));

        let mut called = 0;
        let r = block_on(lexer.consume_char_if(|c| {
            assert_eq!(c, 'o');
            called += 1;
            false
        }));
        assert_eq!(called, 1);
        assert_eq!(r, Ok(None));

        let mut called = 0;
        let c = block_on(lexer.consume_char_if(|c| {
            assert_eq!(c, 'o');
            called += 1;
            true
        }))
        .unwrap()
        .unwrap();
        assert_eq!(called, 1);
        assert_eq!(c.value, 'o');
        assert_eq!(c.location.line.value, "word\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 2);

        block_on(lexer.consume_char_if(|c| {
            assert_eq!(c, 'r');
            true
        }))
        .unwrap()
        .unwrap();
        block_on(lexer.consume_char_if(|c| {
            assert_eq!(c, 'd');
            true
        }))
        .unwrap()
        .unwrap();
        block_on(lexer.consume_char_if(|c| {
            assert_eq!(c, '\n');
            true
        }))
        .unwrap()
        .unwrap();

        // end of input
        let r = block_on(lexer.consume_char_if(|c| {
            panic!("Unexpected call to the decider function: argument={}", c)
        }));
        assert_eq!(r, Ok(None));
    }

    #[test]
    fn lexer_many_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");

        async fn f(l: &mut Lexer) -> Result<Option<SourceChar>> {
            l.consume_char_if(|c| c == 'a').await.map(|o| o.cloned())
        }
        let r = block_on(lexer.many(f)).unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn lexer_many_one() {
        let mut lexer = Lexer::with_source(Source::Unknown, "ab");

        async fn f(l: &mut Lexer) -> Result<Option<SourceChar>> {
            if l.consume_char_if(|c| c == 'a').await?.is_none() {
                return Ok(None);
            }
            l.consume_char_if(|c| c == 'b').await.map(|o| o.cloned())
        }
        let r = block_on(lexer.many(f)).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].value, 'b');
    }

    #[test]
    fn lexer_many_three() {
        let mut lexer = Lexer::with_source(Source::Unknown, "xyxyxyxz");

        async fn f(l: &mut Lexer) -> Result<Option<SourceChar>> {
            if l.consume_char_if(|c| c == 'x').await?.is_none() {
                return Ok(None);
            }
            l.consume_char_if(|c| c == 'y').await.map(|o| o.cloned())
        }
        let r = block_on(lexer.many(f)).unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].value, 'y');
        assert_eq!(r[1].value, 'y');
        assert_eq!(r[2].value, 'y');
    }

    #[test]
    #[should_panic(expected = "Lexer::substitute_alias: begin=0, end=0")]
    fn lexer_substitute_alias_with_invalid_index() {
        let mut lexer = Lexer::with_source(Source::Unknown, "a b");
        let alias = Rc::new(Alias {
            name: "a".to_string(),
            replacement: "".to_string(),
            global: false,
            origin: Location::dummy("dummy".to_string()),
        });
        lexer.substitute_alias(0, &alias);
    }

    #[test]
    fn lexer_substitute_alias_single_line_replacement() {
        let mut lexer = Lexer::with_source(Source::Unknown, "a b");
        let alias = Rc::new(Alias {
            name: "a".to_string(),
            replacement: "lex".to_string(),
            global: false,
            origin: Location::dummy("dummy".to_string()),
        });

        block_on(async {
            let _ = lexer.peek_char().await;
            lexer.consume_char();

            lexer.substitute_alias(0, &alias);

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, 'l');
            assert_eq!(c.location.line.value, "lex");
            assert_eq!(c.location.line.number.get(), 1);
            if let Source::Alias {
                original,
                alias: alias2,
            } = &c.location.line.source
            {
                assert_eq!(original.line.value, "a b");
                assert_eq!(original.line.number.get(), 1);
                assert_eq!(original.line.source, Source::Unknown);
                assert_eq!(original.column.get(), 1);
                assert_eq!(alias2, &alias);
            } else {
                panic!("Wrong source: {:?}", c.location.line.source);
            }
            assert_eq!(c.location.column.get(), 1);
            lexer.consume_char();

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, 'e');
            assert_eq!(c.location.line.value, "lex");
            assert_eq!(c.location.line.number.get(), 1);
            if let Source::Alias {
                original,
                alias: alias2,
            } = &c.location.line.source
            {
                assert_eq!(original.line.value, "a b");
                assert_eq!(original.line.number.get(), 1);
                assert_eq!(original.line.source, Source::Unknown);
                assert_eq!(original.column.get(), 1);
                assert_eq!(alias2, &alias);
            } else {
                panic!("Wrong source: {:?}", c.location.line.source);
            }
            assert_eq!(c.location.column.get(), 2);
            lexer.consume_char();

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, 'x');
            assert_eq!(c.location.line.value, "lex");
            assert_eq!(c.location.line.number.get(), 1);
            if let Source::Alias {
                original,
                alias: alias2,
            } = &c.location.line.source
            {
                assert_eq!(original.line.value, "a b");
                assert_eq!(original.line.number.get(), 1);
                assert_eq!(original.line.source, Source::Unknown);
                assert_eq!(original.column.get(), 1);
                assert_eq!(alias2, &alias);
            } else {
                panic!("Wrong source: {:?}", c.location.line.source);
            }
            assert_eq!(c.location.column.get(), 3);
            lexer.consume_char();

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, ' ');
            assert_eq!(c.location.line.value, "a b");
            assert_eq!(c.location.line.number.get(), 1);
            assert_eq!(c.location.line.source, Source::Unknown);
            assert_eq!(c.location.column.get(), 2);
            lexer.consume_char();
        });
    }

    #[test]
    fn lexer_substitute_alias_multi_line_replacement() {
        let mut lexer = Lexer::with_source(Source::Unknown, " foo b");
        let alias = Rc::new(Alias {
            name: "foo".to_string(),
            replacement: "x\ny".to_string(),
            global: true,
            origin: Location::dummy("loc".to_string()),
        });

        block_on(async {
            for _ in 0usize..4 {
                let _ = lexer.peek_char().await;
                lexer.consume_char();
            }

            lexer.substitute_alias(1, &alias);

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, 'x');
            assert_eq!(c.location.line.value, "x\n");
            assert_eq!(c.location.line.number.get(), 1);
            if let Source::Alias {
                original,
                alias: alias2,
            } = &c.location.line.source
            {
                assert_eq!(original.line.value, " foo b");
                assert_eq!(original.line.number.get(), 1);
                assert_eq!(original.line.source, Source::Unknown);
                assert_eq!(original.column.get(), 2);
                assert_eq!(alias2, &alias);
            } else {
                panic!("Wrong source: {:?}", c.location.line.source);
            }
            assert_eq!(c.location.column.get(), 1);
            lexer.consume_char();

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, '\n');
            assert_eq!(c.location.line.value, "x\n");
            assert_eq!(c.location.line.number.get(), 1);
            if let Source::Alias {
                original,
                alias: alias2,
            } = &c.location.line.source
            {
                assert_eq!(original.line.value, " foo b");
                assert_eq!(original.line.number.get(), 1);
                assert_eq!(original.line.source, Source::Unknown);
                assert_eq!(original.column.get(), 2);
                assert_eq!(alias2, &alias);
            } else {
                panic!("Wrong source: {:?}", c.location.line.source);
            }
            assert_eq!(c.location.column.get(), 2);
            lexer.consume_char();

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, 'y');
            assert_eq!(c.location.line.value, "y");
            assert_eq!(c.location.line.number.get(), 2);
            if let Source::Alias {
                original,
                alias: alias2,
            } = &c.location.line.source
            {
                assert_eq!(original.line.value, " foo b");
                assert_eq!(original.line.number.get(), 1);
                assert_eq!(original.line.source, Source::Unknown);
                assert_eq!(original.column.get(), 2);
                assert_eq!(alias2, &alias);
            } else {
                panic!("Wrong source: {:?}", c.location.line.source);
            }
            assert_eq!(c.location.column.get(), 1);
            lexer.consume_char();

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, ' ');
            assert_eq!(c.location.line.value, " foo b");
            assert_eq!(c.location.line.number.get(), 1);
            assert_eq!(c.location.line.source, Source::Unknown);
            assert_eq!(c.location.column.get(), 5);
            lexer.consume_char();
        });
    }

    #[test]
    fn lexer_substitute_alias_empty_replacement() {
        let mut lexer = Lexer::with_source(Source::Unknown, "x ");
        let alias = Rc::new(Alias {
            name: "x".to_string(),
            replacement: "".to_string(),
            global: false,
            origin: Location::dummy("dummy".to_string()),
        });

        block_on(async {
            let _ = lexer.peek_char().await;
            lexer.consume_char();

            lexer.substitute_alias(0, &alias);

            let c = lexer.peek_char().await.unwrap().unwrap();
            assert_eq!(c.value, ' ');
            assert_eq!(c.location.line.value, "x ");
            assert_eq!(c.location.line.number.get(), 1);
            assert_eq!(c.location.line.source, Source::Unknown);
            assert_eq!(c.location.column.get(), 2);
            lexer.consume_char();
        });
    }

    #[test]
    fn lexer_is_after_blank_ending_alias_index_0() {
        let original = Location::dummy("original".to_string());
        let alias = Rc::new(Alias {
            name: "a".to_string(),
            replacement: " ".to_string(),
            global: false,
            origin: Location::dummy("origin".to_string()),
        });
        let lexer = Lexer::with_source(Source::Alias { original, alias }, "a");
        let result = lexer.is_after_blank_ending_alias(0);
        assert_eq!(result, false);
    }

    #[test]
    fn lexer_is_after_blank_ending_alias_not_blank_ending() {
        let mut lexer = Lexer::with_source(Source::Unknown, "a x");
        let alias = Rc::new(Alias {
            name: "a".to_string(),
            replacement: " b".to_string(),
            global: false,
            origin: Location::dummy("dummy".to_string()),
        });

        block_on(async {
            lexer.peek_char().await.unwrap();
            lexer.consume_char();

            lexer.substitute_alias(0, &alias);

            assert_eq!(lexer.is_after_blank_ending_alias(0), false);
            assert_eq!(lexer.is_after_blank_ending_alias(1), false);
            assert_eq!(lexer.is_after_blank_ending_alias(2), false);
            assert_eq!(lexer.is_after_blank_ending_alias(3), false);
        });
    }

    #[test]
    fn lexer_is_after_blank_ending_alias_blank_ending() {
        let mut lexer = Lexer::with_source(Source::Unknown, "a x");
        let alias = Rc::new(Alias {
            name: "a".to_string(),
            replacement: " b ".to_string(),
            global: false,
            origin: Location::dummy("dummy".to_string()),
        });

        block_on(async {
            lexer.peek_char().await.unwrap();
            lexer.consume_char();

            lexer.substitute_alias(0, &alias);

            assert_eq!(lexer.is_after_blank_ending_alias(0), false);
            assert_eq!(lexer.is_after_blank_ending_alias(1), false);
            assert_eq!(lexer.is_after_blank_ending_alias(2), false);
            assert_eq!(lexer.is_after_blank_ending_alias(3), true);
            assert_eq!(lexer.is_after_blank_ending_alias(4), true);
        });
    }

    #[test]
    fn lexer_inner_program_success() {
        let mut lexer = Lexer::with_source(Source::Unknown, "x y )");
        let source = block_on(lexer.inner_program()).unwrap();
        assert_eq!(source, "x y ");
    }

    #[test]
    fn lexer_inner_program_failure() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<< )");
        let e = block_on(lexer.inner_program()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingHereDocDelimiter)
        );
        assert_eq!(e.location.line.value, "<< )");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 1);
    }
}
