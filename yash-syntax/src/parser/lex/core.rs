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
use super::op::Operator;
use crate::alias::Alias;
use crate::input::Context;
use crate::input::Input;
use crate::input::Memory;
use crate::parser::core::Result;
use crate::parser::error::Error;
use crate::source::lines;
use crate::source::Code;
use crate::source::Location;
use crate::source::Source;
use crate::source::SourceChar;
use crate::syntax::Word;
use std::cell::RefCell;
use std::fmt;
use std::future::Future;
use std::num::NonZeroU64;
use std::ops::Deref;
use std::ops::DerefMut;
use std::pin::Pin;
use std::rc::Rc;
use std::slice::SliceIndex;

/// Returns true if the character is a blank character.
pub fn is_blank(c: char) -> bool {
    // TODO locale
    c != '\n' && c.is_whitespace()
}

/// Result of [`LexerCore::peek_char`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PeekChar<'a> {
    Char(&'a SourceChar),
    EndOfInput(&'a Location),
}

impl<'a> PeekChar<'a> {
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

/// Core part of the lexical analyzer.
struct LexerCore<'a> {
    input: Box<dyn Input + 'a>,
    state: InputState,
    raw_code: Rc<Code>,
    source: Vec<SourceChar>,
    index: usize,
}

impl<'a> LexerCore<'a> {
    /// Creates a new lexer core that reads using the given input function.
    #[must_use]
    fn new(
        input: Box<dyn Input + 'a>,
        start_line_number: NonZeroU64,
        source: Source,
    ) -> LexerCore<'a> {
        LexerCore {
            input,
            raw_code: Rc::new(Code {
                value: RefCell::new(String::new()),
                start_line_number,
                source,
            }),
            state: InputState::Alive,
            source: Vec::new(),
            index: 0,
        }
    }

    /// Peeks the next character, reading the next line if necessary.
    async fn peek_char(&mut self) -> Result<PeekChar<'_>> {
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
                    if line.is_empty() {
                        // End of input
                        self.state = InputState::EndOfInput(Location {
                            code: Rc::clone(&self.raw_code),
                            index: self.index,
                        });
                    } else {
                        // Successful read
                        self.raw_code.value.borrow_mut().push_str(&line);
                        self.source
                            .extend(line.chars().enumerate().map(|(i, value)| SourceChar {
                                value,
                                location: Location {
                                    code: Rc::clone(&self.raw_code),
                                    index: self.index + i,
                                },
                            }))
                    }
                }
                Err(io_error) => {
                    self.state = InputState::Error(Error {
                        cause: io_error.into(),
                        location: Location {
                            code: Rc::clone(&self.raw_code),
                            index: self.index,
                        },
                    });
                }
            }
        }
    }

    /// Consumes the next character.
    ///
    /// This function must be called after [`peek_char`](Lexer::peek_char) has successfully
    /// returned the character. Consuming a character that has not yet been peeked would result
    /// in a panic!
    fn consume_char(&mut self) {
        assert!(
            self.index < self.source.len(),
            "A character must have been peeked before being consumed: index={}",
            self.index
        );
        self.index += 1;
    }

    /// Returns a reference to the character at the given index.
    #[must_use]
    fn peek_char_at(&self, index: usize) -> &SourceChar {
        assert!(
            index <= self.index,
            "The index {} must not be larger than the current index {}",
            index,
            self.index
        );
        &self.source[index]
    }

    /// Returns the current index.
    #[must_use]
    fn index(&self) -> usize {
        self.index
    }

    /// Rewinds the index to the given value.
    fn rewind(&mut self, index: usize) {
        assert!(
            index <= self.index,
            "The new index {} must not be larger than the current index {}",
            index,
            self.index
        );
        self.index = index;
    }

    /// Checks if there is any character that has been read from the input
    /// source but not yet consumed.
    #[must_use]
    fn pending(&self) -> bool {
        self.index < self.source.len()
    }

    /// Clears the internal buffer.
    fn flush(&mut self) {
        let lines = self
            .raw_code
            .value
            .borrow()
            .chars()
            .filter(|&c| c == '\n')
            .count()
            .try_into()
            .unwrap_or(u64::MAX);
        // TODO Use NonZeroU64::saturating_add
        // let start_line_number = self.raw_code.start_line_number.saturating_add(lines);
        let start_line_number = self.raw_code.start_line_number.get().saturating_add(lines);
        let start_line_number = NonZeroU64::new(start_line_number).unwrap();
        self.raw_code = Rc::new(Code {
            value: RefCell::new(String::new()),
            start_line_number,
            source: self.raw_code.source.clone(),
        });
        self.source.clear();
        self.index = 0;
    }

    /// Clears an end-of-input or error status so that the lexer can resume
    /// parsing.
    fn reset(&mut self) {
        self.state = InputState::Alive;
        self.flush();
    }

    /// Extracts a string from the source code.
    fn source_string<I>(&self, i: I) -> String
    where
        I: SliceIndex<[SourceChar], Output = [SourceChar]>,
    {
        self.source[i].iter().map(|c| c.value).collect()
    }

    /// Performs alias substitution.
    fn substitute_alias(&mut self, begin: usize, alias: &Rc<Alias>) {
        let end = self.index;
        assert!(
            begin < end,
            "begin index {} should be less than end index {}",
            begin,
            end
        );

        let original = self.source[begin].location.clone();
        let source = Source::Alias {
            original,
            alias: alias.clone(),
        };
        let mut repl = vec![];
        for line in lines(&alias.replacement, source) {
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
    fn is_after_blank_ending_alias(&self, index: usize) -> bool {
        fn ends_with_blank(s: &str) -> bool {
            s.chars().rev().next().map_or(false, is_blank)
        }
        fn is_same_alias(alias: &Alias, sc: Option<&SourceChar>) -> bool {
            match sc {
                None => false,
                Some(sc) => sc.location.code.source.is_alias_for(&alias.name),
            }
        }

        for index in (0..index).rev() {
            let sc = &self.source[index];

            if !is_blank(sc.value) {
                return false;
            }

            if let Source::Alias { ref alias, .. } = sc.location.code.source {
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
}

impl fmt::Debug for LexerCore<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LexerCore")
            .field("state", &self.state)
            .field("source", &self.source)
            .field("index", &self.index)
            .finish()
        // TODO Call finish_non_exhaustive instead of finish
    }
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
#[derive(Debug)]
pub struct Lexer<'a> {
    // `Lexer` is a thin wrapper around `LexerCore`. `Lexer` delegates most
    // functions to `LexerCore`. `Lexer` adds automatic line-continuation
    // skipping to `LexerCore`.
    core: LexerCore<'a>,
    line_continuation_enabled: bool,
}

impl<'a> Lexer<'a> {
    /// Creates a new lexer that reads using the given input function.
    #[must_use]
    pub fn new(
        input: Box<dyn Input + 'a>,
        start_line_number: NonZeroU64,
        source: Source,
    ) -> Lexer<'a> {
        Lexer {
            core: LexerCore::new(input, start_line_number, source),
            line_continuation_enabled: true,
        }
    }

    /// Creates a new lexer with a fixed source code.
    #[must_use]
    pub fn from_memory(code: &'a str, source: Source) -> Lexer<'a> {
        let line = NonZeroU64::new(1).unwrap();
        Lexer::new(Box::new(Memory::new(code, source.clone())), line, source)
    }

    /// Disables line continuation recognition onward.
    ///
    /// By default, [`peek_char`](Self::peek_char) silently skips line
    /// continuation sequences. When line continuation is disabled, however,
    /// `peek_char` returns characters literally.
    ///
    /// Call [`enable_line_continuation`](Self::enable_line_continuation) to
    /// switch line continuation recognition on.
    ///
    /// This function will panic if line continuation has already been disabled.
    pub fn disable_line_continuation<'b>(&'b mut self) -> PlainLexer<'b, 'a> {
        assert!(
            self.line_continuation_enabled,
            "line continuation already disabled"
        );
        self.line_continuation_enabled = false;
        PlainLexer { lexer: self }
    }

    /// Re-enables line continuation.
    ///
    /// You can pass the `PlainLexer` returned from
    /// [`disable_line_continuation`](Self::disable_line_continuation) to this
    /// function to re-enable line continuation. That is equivalent to dropping
    /// the `PlainLexer` instance, but the code will be more descriptive.
    pub fn enable_line_continuation<'b>(_: PlainLexer<'a, 'b>) {}

    /// Skips line continuation, i.e., a backslash followed by a newline.
    ///
    /// This function does nothing if line continuation has been
    /// [disabled](Self::disable_line_continuation).
    async fn line_continuation(&mut self) -> Result<bool> {
        if !self.line_continuation_enabled {
            return Ok(false);
        }

        let index = self.core.index();
        match self.core.peek_char().await? {
            PeekChar::Char(c) if c.value == '\\' => self.core.consume_char(),
            _ => return Ok(false),
        }

        match self.core.peek_char().await? {
            PeekChar::Char(c) if c.value == '\n' => self.core.consume_char(),
            _ => {
                self.core.rewind(index);
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Peeks the next character.
    ///
    /// If the end of input is reached, `Ok(None)` is returned. On error,
    /// `Err(_)` is returned.
    ///
    /// If line continuation recognition is enabled, combinations of a backslash
    /// and a newline are silently skipped before returning the next character.
    /// Call [`disable_line_continuation`](Self::disable_line_continuation) to
    /// switch off line continuation recognition.
    pub async fn peek_char(&mut self) -> Result<Option<char>> {
        while self.line_continuation().await? {}

        match self.core.peek_char().await? {
            PeekChar::Char(source_char) => Ok(Some(source_char.value)),
            PeekChar::EndOfInput(_) => Ok(None),
        }
    }

    /// Returns the location of the next character.
    ///
    /// If there is no more character (that is, it is the end of input), an imaginary location
    /// is returned that would be returned if a character existed.
    ///
    /// This function requires a mutable reference to `self` since it may need to read a next
    /// line if it is not yet read.
    pub async fn location(&mut self) -> Result<&Location> {
        self.core.peek_char().await.map(|p| p.location())
    }

    /// Consumes the next character.
    ///
    /// This function must be called after [`peek_char`](Lexer::peek_char) has successfully
    /// returned the character. Consuming a character that has not yet been peeked would result
    /// in a panic!
    pub fn consume_char(&mut self) {
        self.core.consume_char()
    }

    /// Returns the position of the next character, counted from zero.
    ///
    /// ```
    /// # use yash_syntax::parser::lex::Lexer;
    /// # use yash_syntax::source::Source;
    /// futures_executor::block_on(async {
    ///     let mut lexer = Lexer::from_memory("abc", Source::Unknown);
    ///     assert_eq!(lexer.index(), 0);
    ///     let _ = lexer.peek_char().await;
    ///     assert_eq!(lexer.index(), 0);
    ///     lexer.consume_char();
    ///     assert_eq!(lexer.index(), 1);
    /// })
    /// ```
    #[must_use]
    pub fn index(&self) -> usize {
        self.core.index()
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
    /// futures_executor::block_on(async {
    ///     let mut lexer = Lexer::from_memory("abc", Source::Unknown);
    ///     let saved_index = lexer.index();
    ///     assert_eq!(lexer.peek_char().await, Ok(Some('a')));
    ///     lexer.consume_char();
    ///     assert_eq!(lexer.peek_char().await, Ok(Some('b')));
    ///     lexer.rewind(saved_index);
    ///     assert_eq!(lexer.peek_char().await, Ok(Some('a')));
    /// })
    /// ```
    pub fn rewind(&mut self, index: usize) {
        self.core.rewind(index)
    }

    /// Checks if there is any character that has been read from the input
    /// source but not yet consumed.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.core.pending()
    }

    /// Clears the internal buffer of the lexer.
    ///
    /// Locations returned from [`location`](Self::location) share a single code
    /// instance that is also retained by the lexer. The code grows long as the
    /// lexer reads more input. To prevent the code from getting too large, you
    /// can call this function that replaces the retained code with a new empty
    /// one. The new code's `start_line_number` will be incremented by the
    /// number of lines in the previous.
    pub fn flush(&mut self) {
        self.core.flush()
    }

    /// Clears an end-of-input or error status so that the lexer can resume
    /// parsing.
    ///
    /// This function will be useful only in an interactive shell where the user
    /// can continue entering commands even after (s)he sends an end-of-input or
    /// is interrupted by a syntax error.
    pub fn reset(&mut self) {
        self.core.reset()
    }

    /// Peeks the next character and, if the given decider function returns true for it,
    /// advances the position.
    ///
    /// Returns the consumed character if the function returned true. Returns `Ok(None)` if it
    /// returned false or there is no more character.
    pub async fn consume_char_if<F>(&mut self, mut f: F) -> Result<Option<&SourceChar>>
    where
        F: FnMut(char) -> bool,
    {
        self.consume_char_if_dyn(&mut f).await
    }

    /// Dynamic version of [`Self::consume_char_if`].
    pub(crate) async fn consume_char_if_dyn(
        &mut self,
        f: &mut dyn FnMut(char) -> bool,
    ) -> Result<Option<&SourceChar>> {
        match self.peek_char().await? {
            Some(c) if f(c) => {
                let index = self.index();
                self.consume_char();
                Ok(Some(self.core.peek_char_at(index)))
            }
            _ => Ok(None),
        }
    }

    /// Extracts a string from the source code.
    ///
    /// This function returns the source code string for the range specified by
    /// the argument. The range must specify a valid index. If the index points
    /// to a character that have not yet read, this function will panic!.
    ///
    /// # Panics
    ///
    /// If the argument index is out of bounds, i.e., pointing to an unread
    /// character.
    pub fn source_string<I>(&self, i: I) -> String
    where
        I: SliceIndex<[SourceChar], Output = [SourceChar]>,
    {
        self.core.source_string(i)
    }

    /// Performs alias substitution right before the current position.
    ///
    /// This function must be called just after a [word](WordLexer::word) has been parsed that
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
        self.core.substitute_alias(begin, alias)
    }

    /// Tests if the given index is after the replacement string of alias
    /// substitution that ends with a blank.
    ///
    /// # Panics
    ///
    /// If `index` is larger than the currently read index.
    pub fn is_after_blank_ending_alias(&self, index: usize) -> bool {
        self.core.is_after_blank_ending_alias(index)
    }

    /// Parses an optional compound list that is the content of a command
    /// substitution.
    ///
    /// This function consumes characters until a token that cannot be the
    /// beginning of an and-or list is found and returns the string that was
    /// consumed.
    pub async fn inner_program(&mut self) -> Result<String> {
        let begin = self.index();

        let aliases = Default::default();
        let mut parser = super::super::Parser::new(self, &aliases);
        parser.maybe_compound_list().await?;

        let end = parser.peek_token().await?.index;
        self.rewind(end);

        Ok(self.core.source_string(begin..end))
    }

    /// Like [`Lexer::inner_program`], but returns the future in a pinned box.
    pub fn inner_program_boxed(&mut self) -> Pin<Box<dyn Future<Output = Result<String>> + '_>> {
        Box::pin(self.inner_program())
    }
}

/// Reference to [`Lexer`] with line continuation disabled.
///
/// This struct implements the RAII pattern for temporarily disabling line
/// continuation. When you disable the line continuation of a lexer, you get an
/// instance of `PlainLexer`. You can access the original lexer via the
/// `PlainLexer` until you drop it, when the line continuation is automatically
/// re-enabled.
#[derive(Debug)]
#[must_use = "You must retain the PlainLexer to keep line continuation disabled"]
pub struct PlainLexer<'a, 'b> {
    lexer: &'a mut Lexer<'b>,
}

impl<'a, 'b> Deref for PlainLexer<'a, 'b> {
    type Target = Lexer<'b>;
    fn deref(&self) -> &Lexer<'b> {
        self.lexer
    }
}

impl<'a, 'b> DerefMut for PlainLexer<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Lexer<'b> {
        self.lexer
    }
}

impl Drop for PlainLexer<'_, '_> {
    fn drop(&mut self) {
        self.lexer.line_continuation_enabled = true;
    }
}

/// Context in which a [word](crate::syntax::Word) is parsed.
///
/// The parse of the word of a [switch](crate::syntax::Switch) depends on
/// whether the parameter expansion containing the switch is part of a text or a
/// word. A `WordContext` value is used to decide the behavior of the lexer.
///
/// Parser functions that depend on the context are implemented in
/// [`WordLexer`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WordContext {
    /// The text unit being parsed is part of a [text](crate::syntax::Text).
    Text,
    /// The text unit being parsed is part of a [word](crate::syntax::Word).
    Word,
}

/// Lexer with additional information for parsing [texts](crate::syntax::Text)
/// and [words](crate::syntax::Word).
#[derive(Debug)]
pub struct WordLexer<'a, 'b> {
    pub lexer: &'a mut Lexer<'b>,
    pub context: WordContext,
}

impl<'a, 'b> Deref for WordLexer<'a, 'b> {
    type Target = Lexer<'b>;
    fn deref(&self) -> &Lexer<'b> {
        self.lexer
    }
}

impl<'a, 'b> DerefMut for WordLexer<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Lexer<'b> {
        self.lexer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorCause;
    use crate::parser::error::SyntaxError;
    use assert_matches::assert_matches;
    use futures_executor::block_on;

    #[test]
    fn lexer_core_peek_char_empty_source() {
        let input = Memory::new("", Source::Unknown);
        let line = NonZeroU64::new(32).unwrap();
        let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
        let result = block_on(lexer.peek_char());
        assert_matches!(result, Ok(PeekChar::EndOfInput(location)) => {
            assert_eq!(*location.code.value.borrow(), "");
            assert_eq!(location.code.start_line_number, line);
            assert_eq!(location.code.source, Source::Unknown);
            assert_eq!(location.index, 0);
        });
    }

    #[test]
    fn lexer_core_peek_char_io_error() {
        #[derive(Debug)]
        struct Failing;
        impl fmt::Display for Failing {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "Failing")
            }
        }
        impl std::error::Error for Failing {}
        #[async_trait::async_trait(?Send)]
        impl Input for Failing {
            async fn next_line(&mut self, _: &Context) -> crate::input::Result {
                Err(std::io::Error::new(std::io::ErrorKind::Other, Failing))
            }
        }
        let line = NonZeroU64::new(42).unwrap();
        let mut lexer = LexerCore::new(Box::new(Failing), line, Source::Unknown);

        let e = block_on(lexer.peek_char()).unwrap_err();
        assert_matches!(e.cause, ErrorCause::Io(io_error) => {
            assert_eq!(io_error.kind(), std::io::ErrorKind::Other);
        });
        assert_eq!(*e.location.code.value.borrow(), "");
        assert_eq!(e.location.code.start_line_number, line);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 0);
    }

    #[test]
    fn lexer_core_consume_char_success() {
        let input = Memory::new("a\nb", Source::Unknown);
        let line = NonZeroU64::new(1).unwrap();
        let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);

        let result = block_on(lexer.peek_char());
        assert_matches!(result, Ok(PeekChar::Char(c)) => {
            assert_eq!(c.value, 'a');
            assert_eq!(*c.location.code.value.borrow(), "a\n");
            assert_eq!(c.location.code.start_line_number, line);
            assert_eq!(c.location.code.source, Source::Unknown);
            assert_eq!(c.location.index, 0);
        });
        assert_matches!(result, Ok(PeekChar::Char(c)) => {
            assert_eq!(c.value, 'a');
            assert_eq!(*c.location.code.value.borrow(), "a\n");
            assert_eq!(c.location.code.start_line_number, line);
            assert_eq!(c.location.code.source, Source::Unknown);
            assert_eq!(c.location.index, 0);
        });
        lexer.consume_char();

        let result = block_on(lexer.peek_char());
        assert_matches!(result, Ok(PeekChar::Char(c)) => {
            assert_eq!(c.value, '\n');
            assert_eq!(*c.location.code.value.borrow(), "a\n");
            assert_eq!(c.location.code.start_line_number, line);
            assert_eq!(c.location.code.source, Source::Unknown);
            assert_eq!(c.location.index, 1);
        });
        lexer.consume_char();

        let result = block_on(lexer.peek_char());
        assert_matches!(result, Ok(PeekChar::Char(c)) => {
            assert_eq!(c.value, 'b');
            assert_eq!(*c.location.code.value.borrow(), "a\nb");
            assert_eq!(c.location.code.start_line_number.get(), 1);
            assert_eq!(c.location.code.source, Source::Unknown);
            assert_eq!(c.location.index, 2);
        });
        lexer.consume_char();

        let result = block_on(lexer.peek_char());
        assert_matches!(result, Ok(PeekChar::EndOfInput(location)) => {
            assert_eq!(*location.code.value.borrow(), "a\nb");
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(location.code.source, Source::Unknown);
            assert_eq!(location.index, 3);
        });
    }

    #[test]
    #[should_panic(expected = "A character must have been peeked before being consumed: index=0")]
    fn lexer_core_consume_char_panic() {
        let input = Memory::new("a", Source::Unknown);
        let line = NonZeroU64::new(1).unwrap();
        let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
        lexer.consume_char();
    }

    #[test]
    fn lexer_core_peek_char_at() {
        let input = Memory::new("a\nb", Source::Unknown);
        let line = NonZeroU64::new(1).unwrap();
        let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);

        let c0 = assert_matches!(block_on(lexer.peek_char()), Ok(PeekChar::Char(c)) => c.clone());
        lexer.consume_char();

        let c1 = assert_matches!(block_on(lexer.peek_char()), Ok(PeekChar::Char(c)) => c.clone());
        lexer.consume_char();

        let c2 = assert_matches!(block_on(lexer.peek_char()), Ok(PeekChar::Char(c)) => c.clone());

        assert_eq!(lexer.peek_char_at(0), &c0);
        assert_eq!(lexer.peek_char_at(1), &c1);
        assert_eq!(lexer.peek_char_at(2), &c2);
    }

    #[test]
    fn lexer_core_index() {
        let input = Memory::new("a\nb", Source::Unknown);
        let line = NonZeroU64::new(1).unwrap();
        let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);

        assert_eq!(lexer.index(), 0);
        block_on(lexer.peek_char()).unwrap();
        assert_eq!(lexer.index(), 0);
        lexer.consume_char();

        assert_eq!(lexer.index(), 1);
        block_on(lexer.peek_char()).unwrap();
        lexer.consume_char();

        assert_eq!(lexer.index(), 2);
        block_on(lexer.peek_char()).unwrap();
        lexer.consume_char();

        assert_eq!(lexer.index(), 3);
    }

    #[test]
    fn lexer_core_rewind_success() {
        let input = Memory::new("abc", Source::Unknown);
        let line = NonZeroU64::new(1).unwrap();
        let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
        lexer.rewind(0);
        assert_eq!(lexer.index(), 0);

        block_on(async {
            let _ = lexer.peek_char().await;
            lexer.consume_char();
            let _ = lexer.peek_char().await;
            lexer.consume_char();
            lexer.rewind(0);

            let result = lexer.peek_char().await;
            assert_matches!(result, Ok(PeekChar::Char(c)) => {
                assert_eq!(c.value, 'a');
                assert_eq!(*c.location.code.value.borrow(), "abc");
                assert_eq!(c.location.code.start_line_number, line);
                assert_eq!(c.location.code.source, Source::Unknown);
                assert_eq!(c.location.index, 0);
            });
        });
    }

    #[test]
    #[should_panic(expected = "The new index 1 must not be larger than the current index 0")]
    fn lexer_core_rewind_invalid_index() {
        let input = Memory::new("abc", Source::Unknown);
        let line = NonZeroU64::new(1).unwrap();
        let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
        lexer.rewind(1);
    }

    #[test]
    fn lexer_core_source_string() {
        let input = Memory::new("ab\ncd", Source::Unknown);
        let line = NonZeroU64::new(1).unwrap();
        let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
        block_on(async {
            for _ in 0..4 {
                let _ = lexer.peek_char().await;
                lexer.consume_char();
            }
        });

        let result = lexer.source_string(1..4);
        assert_eq!(result, "b\nc");
    }

    #[test]
    #[should_panic(expected = "begin index 0 should be less than end index 0")]
    fn lexer_core_substitute_alias_with_invalid_index() {
        let input = Memory::new("a b", Source::Unknown);
        let line = NonZeroU64::new(1).unwrap();
        let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
        let alias = Rc::new(Alias {
            name: "a".to_string(),
            replacement: "".to_string(),
            global: false,
            origin: Location::dummy("dummy"),
        });
        lexer.substitute_alias(0, &alias);
    }

    #[test]
    fn lexer_core_substitute_alias_single_line_replacement() {
        let input = Memory::new("a b", Source::Unknown);
        let line = NonZeroU64::new(1).unwrap();
        let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
        let alias = Rc::new(Alias {
            name: "a".to_string(),
            replacement: "lex".to_string(),
            global: false,
            origin: Location::dummy("dummy"),
        });

        block_on(async {
            let _ = lexer.peek_char().await;
            lexer.consume_char();

            lexer.substitute_alias(0, &alias);

            assert_matches!(lexer.peek_char().await, Ok(PeekChar::Char(c)) => {
                assert_eq!(c.value, 'l');
                assert_eq!(*c.location.code.value.borrow(), "lex");
                assert_eq!(c.location.code.start_line_number.get(), 1);
                assert_matches!(&c.location.code.source,
                    Source::Alias { original, alias: alias2 } => {
                    assert_eq!(*original.code.value.borrow(), "a b");
                    assert_eq!(original.code.start_line_number, line);
                    assert_eq!(original.code.source, Source::Unknown);
                    assert_eq!(original.index, 0);
                    assert_eq!(alias2, &alias);
                });
                assert_eq!(c.location.index, 0);
            });
            lexer.consume_char();

            assert_matches!(lexer.peek_char().await, Ok(PeekChar::Char(c)) => {
                assert_eq!(c.value, 'e');
                assert_eq!(*c.location.code.value.borrow(), "lex");
                assert_eq!(c.location.code.start_line_number, line);
                assert_matches!(&c.location.code.source,
                    Source::Alias { original, alias: alias2 } => {
                    assert_eq!(*original.code.value.borrow(), "a b");
                    assert_eq!(original.code.start_line_number, line);
                    assert_eq!(original.code.source, Source::Unknown);
                    assert_eq!(original.index, 0);
                    assert_eq!(alias2, &alias);
                });
                assert_eq!(c.location.index, 1);
            });
            lexer.consume_char();

            assert_matches!(lexer.peek_char().await, Ok(PeekChar::Char(c)) => {
                assert_eq!(c.value, 'x');
                assert_eq!(*c.location.code.value.borrow(), "lex");
                assert_eq!(c.location.code.start_line_number, line);
                assert_matches!(&c.location.code.source,
                    Source::Alias { original, alias: alias2 } => {
                    assert_eq!(*original.code.value.borrow(), "a b");
                    assert_eq!(original.code.start_line_number, line);
                    assert_eq!(original.code.source, Source::Unknown);
                    assert_eq!(original.index, 0);
                    assert_eq!(alias2, &alias);
                });
                assert_eq!(c.location.index, 2);
            });
            lexer.consume_char();

            assert_matches!(lexer.peek_char().await, Ok(PeekChar::Char(c)) => {
                assert_eq!(c.value, ' ');
                assert_eq!(*c.location.code.value.borrow(), "a b");
                assert_eq!(c.location.code.start_line_number, line);
                assert_eq!(c.location.code.source, Source::Unknown);
                assert_eq!(c.location.index, 1);
            });
            lexer.consume_char();
        });
    }

    #[test]
    fn lexer_core_substitute_alias_multi_line_replacement() {
        let input = Memory::new(" foo b", Source::Unknown);
        let line = NonZeroU64::new(1).unwrap();
        let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
        let alias = Rc::new(Alias {
            name: "foo".to_string(),
            replacement: "x\ny".to_string(),
            global: true,
            origin: Location::dummy("loc"),
        });

        block_on(async {
            for _ in 0..4 {
                let _ = lexer.peek_char().await;
                lexer.consume_char();
            }

            lexer.substitute_alias(1, &alias);

            assert_matches!(lexer.peek_char().await, Ok(PeekChar::Char(c)) => {
                assert_eq!(c.value, 'x');
                assert_eq!(*c.location.code.value.borrow(), "x\n");
                assert_eq!(c.location.code.start_line_number, line);
                assert_matches!(&c.location.code.source,
                    Source::Alias { original, alias: alias2 } => {
                    assert_eq!(*original.code.value.borrow(), " foo b");
                    assert_eq!(original.code.start_line_number, line);
                    assert_eq!(original.code.source, Source::Unknown);
                    assert_eq!(original.index, 1);
                    assert_eq!(alias2, &alias);
                });
                assert_eq!(c.location.index, 0);
            });
            lexer.consume_char();

            assert_matches!(lexer.peek_char().await, Ok(PeekChar::Char(c)) => {
                assert_eq!(c.value, '\n');
                assert_eq!(*c.location.code.value.borrow(), "x\n");
                assert_eq!(c.location.code.start_line_number, line);
                assert_matches!(&c.location.code.source,
                    Source::Alias { original, alias: alias2 } => {
                    assert_eq!(*original.code.value.borrow(), " foo b");
                    assert_eq!(original.code.start_line_number, line);
                    assert_eq!(original.code.source, Source::Unknown);
                    assert_eq!(original.index, 1);
                    assert_eq!(alias2, &alias);
                });
                assert_eq!(c.location.index, 1);
            });
            lexer.consume_char();

            assert_matches!(lexer.peek_char().await, Ok(PeekChar::Char(c)) => {
                assert_eq!(c.value, 'y');
                assert_eq!(*c.location.code.value.borrow(), "y");
                assert_eq!(c.location.code.start_line_number.get(), 2);
                assert_matches!(&c.location.code.source, Source::Alias { original, alias: alias2 } => {
                    assert_eq!(*original.code.value.borrow(), " foo b");
                    assert_eq!(original.code.start_line_number, line);
                    assert_eq!(original.code.source, Source::Unknown);
                    assert_eq!(original.index, 1);
                    assert_eq!(alias2, &alias);
                });
                assert_eq!(c.location.index, 0);
            });
            lexer.consume_char();

            assert_matches!(lexer.peek_char().await, Ok(PeekChar::Char(c)) => {
                assert_eq!(c.value, ' ');
                assert_eq!(*c.location.code.value.borrow(), " foo b");
                assert_eq!(c.location.code.start_line_number, line);
                assert_eq!(c.location.code.source, Source::Unknown);
                assert_eq!(c.location.index, 4);
            });
            lexer.consume_char();
        });
    }

    #[test]
    fn lexer_core_substitute_alias_empty_replacement() {
        block_on(async {
            let input = Memory::new("x ", Source::Unknown);
            let line = NonZeroU64::new(1).unwrap();
            let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
            let alias = Rc::new(Alias {
                name: "x".to_string(),
                replacement: "".to_string(),
                global: false,
                origin: Location::dummy("dummy"),
            });

            let _ = lexer.peek_char().await;
            lexer.consume_char();

            lexer.substitute_alias(0, &alias);

            assert_matches!(lexer.peek_char().await, Ok(PeekChar::Char(c)) => {
                assert_eq!(c.value, ' ');
                assert_eq!(*c.location.code.value.borrow(), "x ");
                assert_eq!(c.location.code.start_line_number, line);
                assert_eq!(c.location.code.source, Source::Unknown);
                assert_eq!(c.location.index, 1);
            });
        });
    }

    #[test]
    fn lexer_core_is_after_blank_ending_alias_index_0() {
        let original = Location::dummy("original");
        let alias = Rc::new(Alias {
            name: "a".to_string(),
            replacement: " ".to_string(),
            global: false,
            origin: Location::dummy("origin"),
        });
        let input = Memory::new("a", Source::Alias { original, alias });
        let line = NonZeroU64::new(1).unwrap();
        let lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
        assert!(!lexer.is_after_blank_ending_alias(0));
    }

    #[test]
    fn lexer_core_is_after_blank_ending_alias_not_blank_ending() {
        block_on(async {
            let input = Memory::new("a x", Source::Unknown);
            let line = NonZeroU64::new(1).unwrap();
            let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
            let alias = Rc::new(Alias {
                name: "a".to_string(),
                replacement: " b".to_string(),
                global: false,
                origin: Location::dummy("dummy"),
            });

            lexer.peek_char().await.unwrap();
            lexer.consume_char();

            lexer.substitute_alias(0, &alias);

            assert!(!lexer.is_after_blank_ending_alias(0));
            assert!(!lexer.is_after_blank_ending_alias(1));
            assert!(!lexer.is_after_blank_ending_alias(2));
            assert!(!lexer.is_after_blank_ending_alias(3));
        });
    }

    #[test]
    fn lexer_core_is_after_blank_ending_alias_blank_ending() {
        block_on(async {
            let input = Memory::new("a x", Source::Unknown);
            let line = NonZeroU64::new(1).unwrap();
            let mut lexer = LexerCore::new(Box::new(input), line, Source::Unknown);
            let alias = Rc::new(Alias {
                name: "a".to_string(),
                replacement: " b ".to_string(),
                global: false,
                origin: Location::dummy("dummy"),
            });

            lexer.peek_char().await.unwrap();
            lexer.consume_char();

            lexer.substitute_alias(0, &alias);

            assert!(!lexer.is_after_blank_ending_alias(0));
            assert!(!lexer.is_after_blank_ending_alias(1));
            assert!(!lexer.is_after_blank_ending_alias(2));
            assert!(lexer.is_after_blank_ending_alias(3));
            assert!(lexer.is_after_blank_ending_alias(4));
        });
    }

    #[test]
    fn lexer_with_empty_source() {
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_peek_char_with_line_continuation_enabled_stopping_on_non_backslash() {
        let mut lexer = Lexer::from_memory("\\\n\n\\", Source::Unknown);
        assert_eq!(block_on(lexer.peek_char()), Ok(Some('\n')));
        assert_eq!(lexer.index(), 2);
    }

    #[test]
    fn lexer_peek_char_with_line_continuation_enabled_stopping_on_non_newline() {
        let mut lexer = Lexer::from_memory("\\\n\\\n\\\n\\\\", Source::Unknown);
        assert_eq!(block_on(lexer.peek_char()), Ok(Some('\\')));
        assert_eq!(lexer.index(), 6);
    }

    #[test]
    fn lexer_peek_char_with_line_continuation_disabled() {
        let mut lexer = Lexer::from_memory("\\\n\\\n\\\\", Source::Unknown);
        let mut lexer = lexer.disable_line_continuation();
        assert_eq!(block_on(lexer.peek_char()), Ok(Some('\\')));
        assert_eq!(lexer.index(), 0);
    }

    #[test]
    fn lexer_flush() {
        block_on(async {
            let mut lexer = Lexer::from_memory(" \n\n\t\n", Source::Unknown);
            let location_1 = lexer.location().await.unwrap().clone();
            assert_eq!(*location_1.code.value.borrow(), " \n");

            lexer.consume_char();
            lexer.peek_char().await.unwrap();
            lexer.consume_char();
            lexer.peek_char().await.unwrap();
            lexer.consume_char();
            lexer.flush();
            lexer.peek_char().await.unwrap();
            lexer.consume_char();

            let location_2 = lexer.location().await.unwrap().clone();

            assert_eq!(*location_1.code.value.borrow(), " \n\n");
            assert_eq!(location_1.code.start_line_number.get(), 1);
            assert_eq!(location_1.code.source, Source::Unknown);
            assert_eq!(location_1.index, 0);
            assert_eq!(*location_2.code.value.borrow(), "\t\n");
            assert_eq!(location_2.code.start_line_number.get(), 3);
            assert_eq!(location_2.code.source, Source::Unknown);
            assert_eq!(location_2.index, 1);
        });
    }

    #[test]
    fn lexer_consume_char_if() {
        let mut lexer = Lexer::from_memory("word\n", Source::Unknown);

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
        assert_eq!(*c.location.code.value.borrow(), "word\n");
        assert_eq!(c.location.code.start_line_number.get(), 1);
        assert_eq!(c.location.code.source, Source::Unknown);
        assert_eq!(c.location.index, 0);

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
        assert_eq!(*c.location.code.value.borrow(), "word\n");
        assert_eq!(c.location.code.start_line_number.get(), 1);
        assert_eq!(c.location.code.source, Source::Unknown);
        assert_eq!(c.location.index, 1);

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
    fn lexer_inner_program_success() {
        let mut lexer = Lexer::from_memory("x y )", Source::Unknown);
        let source = block_on(lexer.inner_program()).unwrap();
        assert_eq!(source, "x y ");
    }

    #[test]
    fn lexer_inner_program_failure() {
        let mut lexer = Lexer::from_memory("<< )", Source::Unknown);
        let e = block_on(lexer.inner_program()).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::Syntax(SyntaxError::MissingHereDocDelimiter)
        );
        assert_eq!(*e.location.code.value.borrow(), "<< )");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 3);
    }
}
