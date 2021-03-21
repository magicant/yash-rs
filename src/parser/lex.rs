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

// mod heredoc; // See below
mod op;

mod core {

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
                    InputState::EndOfInput(ref location) => {
                        return Ok(PeekChar::EndOfInput(location))
                    }
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
        /// futures::executor::block_on(async {
        ///     let mut lexer = yash::parser::lex::Lexer::with_source(
        ///         yash::source::Source::Unknown, "abc");
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
        /// futures::executor::block_on(async {
        ///     let mut lexer = yash::parser::lex::Lexer::with_source(
        ///         yash::source::Source::Unknown, "abc");
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
        pub fn inner_program_boxed(
            &mut self,
        ) -> Pin<Box<dyn Future<Output = Result<String>> + '_>> {
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
}

pub use self::core::*;
pub use self::heredoc::PartialHereDoc;
pub use self::op::is_operator_char;

use self::keyword::Keyword;
use self::op::Trie;
use self::op::OPERATORS;
use crate::parser::core::Error;
use crate::parser::core::Result;
use crate::parser::core::SyntaxError;
use crate::source::Location;
use crate::source::SourceChar;
use crate::syntax::*;
use std::convert::TryFrom;
use std::future::Future;
use std::pin::Pin;

/// Tests whether the given character is a token delimiter.
///
/// A character is a token delimiter if it is either a whitespace or [operator](is_operator_char).
pub fn is_token_delimiter_char(c: char) -> bool {
    is_operator_char(c) || is_blank(c)
}

/// Return type for [`Lexer::operator_tail`]
struct OperatorTail {
    pub operator: Operator,
    pub location: Location,
    pub reversed_key: Vec<char>,
}

impl Lexer {
    /// Skips a character if the given function returns true for it.
    ///
    /// Returns `Ok(true)` if the character was skipped, `Ok(false)` if the function returned
    /// false, and `Err(_)` if an error occurred, respectively.
    ///
    /// `skip_if` is a simpler version of [`consume_char_if`](Lexer::consume_char_if).
    pub async fn skip_if<F>(&mut self, f: F) -> Result<bool>
    where
        F: FnOnce(char) -> bool,
    {
        Ok(self.consume_char_if(f).await?.is_some())
    }

    /// Skips line continuations, if any.
    pub async fn line_continuations(&mut self) -> Result<()> {
        async fn line_continuation(this: &mut Lexer) -> Result<Option<()>> {
            let ok = this.skip_if(|c| c == '\\').await? && this.skip_if(|c| c == '\n').await?;
            Ok(if ok { Some(()) } else { None })
        }
        self.many(line_continuation).await.map(|_| ())
    }

    /// Skips blank characters until reaching a non-blank.
    ///
    /// This function also skips line continuations.
    pub async fn skip_blanks(&mut self) -> Result<()> {
        loop {
            self.line_continuations().await?;
            if !self.skip_if(is_blank).await? {
                break Ok(());
            }
        }
    }

    /// Skips a comment, if any.
    ///
    /// A comment ends just before a newline. The newline is *not* part of the comment.
    ///
    /// This function does not recognize any line continuations.
    pub async fn skip_comment(&mut self) -> Result<()> {
        if self.skip_if(|c| c == '#').await? {
            while self.skip_if(|c| c != '\n').await? {}
        }
        Ok(())
    }

    /// Skips blank characters and a comment, if any.
    ///
    /// This function also skips line continuations between blanks. It is the same as
    /// [`skip_blanks`](Lexer::skip_blanks) followed by [`skip_comment`](Lexer::skip_comment).
    pub async fn skip_blanks_and_comment(&mut self) -> Result<()> {
        self.skip_blanks().await?;
        self.skip_comment().await
    }

    /// Parses an operator that matches a key in the given trie, if any.
    fn operator_tail(
        &mut self,
        trie: Trie,
    ) -> Pin<Box<dyn Future<Output = Result<Option<OperatorTail>>> + '_>> {
        Box::pin(async move {
            if trie.is_empty() {
                return Ok(None);
            }

            self.line_continuations().await?;

            let sc = match self.peek_char().await? {
                None => return Ok(None),
                Some(sc) => sc.clone(),
            };
            let edge = match trie.edge(sc.value) {
                None => return Ok(None),
                Some(edge) => edge,
            };

            let old_index = self.index();
            self.consume_char();

            if let Some(OperatorTail {
                operator,
                location: _,
                mut reversed_key,
            }) = self.operator_tail(edge.next).await?
            {
                reversed_key.push(sc.value);
                return Ok(Some(OperatorTail {
                    operator,
                    location: sc.location,
                    reversed_key,
                }));
            }

            match edge.value {
                None => {
                    self.rewind(old_index);
                    Ok(None)
                }
                Some(operator) => Ok(Some(OperatorTail {
                    operator,
                    location: sc.location,
                    reversed_key: vec![sc.value],
                })),
            }
        })
    }

    /// Parses an operator token.
    pub async fn operator(&mut self) -> Result<Option<Token>> {
        let index = self.index();
        self.operator_tail(OPERATORS).await.map(|o| {
            o.map(|ot| {
                let OperatorTail {
                    operator,
                    location,
                    reversed_key,
                } = ot;
                let units = reversed_key
                    .into_iter()
                    .rev()
                    .map(|c| Unquoted(Literal(c)))
                    .collect::<Vec<_>>();
                let word = Word { units, location };
                let id = TokenId::Operator(operator);
                Token { word, id, index }
            })
        })
    }

    /// Parses a command substitution of the form `$(...)`.
    ///
    /// The initial `$` must have been consumed before calling this function.
    /// In this function, the next character is examined to see if it begins a
    /// command substitution. If it is `(`, the following characters are parsed
    /// as commands to find a matching `)`, which will be consumed before this
    /// function returns. Otherwise, no characters are consumed and the return
    /// value is `Ok(None)`.
    ///
    /// `opening_location` should be the location of the initial `$`. It is used
    /// to construct the result, but this function does not check if it actually
    /// is a location of `$`.
    ///
    /// This function does not consume line continuations between `$` and `(`.
    /// Line continuations should have been consumed beforehand.
    ///
    /// # Errors
    ///
    /// - [SyntaxError::UnclosedCommandSubstitution]
    pub async fn command_substitution(
        &mut self,
        opening_location: Location,
    ) -> Result<Option<TextUnit>> {
        if !self.skip_if(|c| c == '(').await? {
            return Ok(None);
        }

        let content = self.inner_program_boxed().await?;

        if !self.skip_if(|c| c == ')').await? {
            // TODO Return a better error depending on the token id of the next token
            let location = self.location().await?.clone();
            return Err(Error {
                cause: SyntaxError::UnclosedCommandSubstitution { opening_location }.into(),
                location,
            });
        }

        let location = opening_location;
        Ok(Some(TextUnit::CommandSubst { content, location }))
    }

    /// Parses a word unit that starts with `$`.
    ///
    /// If the next character is `$`, a parameter expansion, command
    /// substitution, or arithmetic expansion is parsed. Otherwise, no
    /// characters are consumed and the return value is `Ok(None)`.
    ///
    /// # Errors
    ///
    /// - Propagated from [`Lexer::command_substitution`]
    pub async fn dollar_word_unit(&mut self) -> Result<Option<TextUnit>> {
        let index = self.index();
        let location = match self.consume_char_if(|c| c == '$').await? {
            None => return Ok(None),
            Some(c) => c.location.clone(),
        };

        // TODO line continuations following $
        // TODO braced parameter expansion
        // TODO non-braced parameter expansion
        // TODO arithmetic expansion
        if let Some(result) = self.command_substitution(location).await? {
            return Ok(Some(result));
        }

        self.rewind(index);
        Ok(None)
    }

    // TODO Backquote command substitution.
    /// Parses a [`TextUnit`].
    ///
    /// This function parses a literal character, backslash-escaped character, or
    /// [dollar word unit](Lexer::dollar_word_unit), optionally preceded by line
    /// continuations.
    ///
    /// `is_delimiter` is a function that decides if a character is a delimiter.
    /// An unquoted character is parsed only if `is_delimiter` returns false for
    /// it.
    ///
    /// `is_escapable` decides if a character can be escaped by a backslash. When
    /// `is_escapable` returns false, the preceding backslash is considered
    /// literal.
    pub async fn text_unit<F, G>(
        &mut self,
        is_delimiter: F,
        is_escapable: G,
    ) -> Result<Option<TextUnit>>
    where
        F: FnOnce(char) -> bool,
        G: FnOnce(char) -> bool,
    {
        self.line_continuations().await?;

        if self.skip_if(|c| c == '\\').await? {
            if let Some(c) = self.consume_char_if(is_escapable).await? {
                return Ok(Some(Backslashed(c.value)));
            } else {
                // The backslash is trying to escape the end of input!
                // We treat the backslash literally. (Should we return an error?)
                return Ok(Some(Literal('\\')));
            }
        }

        if let Some(u) = self.dollar_word_unit().await? {
            return Ok(Some(u));
        }

        if let Some(sc) = self.consume_char_if(|c| !is_delimiter(c)).await? {
            return Ok(Some(Literal(sc.value)));
        }

        Ok(None)
    }

    /// Parses a text, i.e., a (possibly empty) sequence of [`TextUnit`]s.
    ///
    /// `is_delimiter` tests if an unquoted character is a delimiter. When
    /// `is_delimiter` returns true, the parser ends parsing and returns the text
    /// up to the character as a result.
    ///
    /// `is_escapable` tests if a backslash can escape a character. When the
    /// parser founds an unquoted backslash, the next character is passed to
    /// `is_escapable`. If `is_escapable` returns true, the backslash is treated
    /// as a valid escape (`TextUnit::Backslashed`). Otherwise, it ia a
    /// literal (`TextUnit::Literal`).
    pub async fn text<F, G>(&mut self, mut is_delimiter: F, mut is_escapable: G) -> Result<Text>
    where
        F: FnMut(char) -> bool,
        G: FnMut(char) -> bool,
    {
        let mut units = vec![];

        while let Some(unit) = self.text_unit(&mut is_delimiter, &mut is_escapable).await? {
            units.push(unit);
        }

        Ok(Text(units))
    }

    /// Parses a single-quoted string.
    ///
    /// The opening `'` must have been consumed before calling this function.
    /// The closing `'` is consumed in this function.
    ///
    /// `opening_location` should be the location of the opening `'`. It is used
    /// to construct an error value, but this function does not check if it
    /// actually is a location of `'`.
    async fn single_quote(&mut self, opening_location: Location) -> Result<WordUnit> {
        let mut content = String::new();
        loop {
            match self.consume_char_if(|_| true).await? {
                Some(&SourceChar { value: '\'', .. }) => return Ok(SingleQuote(content)),
                Some(&SourceChar { value, .. }) => content.push(value),
                None => {
                    let cause = SyntaxError::UnclosedSingleQuote { opening_location }.into();
                    let location = self.location().await?.clone();
                    return Err(Error { cause, location });
                }
            }
        }
    }

    /// Parses a double-quoted string.
    ///
    /// The opening `"` must have been consumed before calling this function.
    /// The closing `"` is consumed in this function.
    ///
    /// `opening_location` should be the location of the opening `"`. It is used
    /// to construct an error value, but this function does not check if it
    /// actually is a location of `"`.
    async fn double_quote(&mut self, opening_location: Location) -> Result<WordUnit> {
        fn is_delimiter(c: char) -> bool {
            c == '"'
        }
        fn is_escapable(c: char) -> bool {
            matches!(c, '$' | '`' | '"' | '\\')
        }

        let content = self.text(is_delimiter, is_escapable).await?;

        if self.skip_if(|c| c == '"').await? {
            Ok(DoubleQuote(content))
        } else {
            let cause = SyntaxError::UnclosedDoubleQuote { opening_location }.into();
            let location = self.location().await?.clone();
            Err(Error { cause, location })
        }
    }

    /// Parses a word unit.
    ///
    /// `is_delimiter` is a function that decides a character is a delimiter. An
    /// unquoted character is parsed only if `is_delimiter` returns false for it.
    pub async fn word_unit<F>(&mut self, is_delimiter: F) -> Result<Option<WordUnit>>
    where
        F: FnOnce(char) -> bool,
    {
        // TODO Parse line continuations before the word unit
        // TODO Parse other types of word units
        match self.consume_char_if(|c| c == '\'' || c == '"').await? {
            None => Ok(self.text_unit(is_delimiter, |_| true).await?.map(Unquoted)),
            Some(sc) => {
                let location = sc.location.clone();
                match sc.value {
                    '\'' => self.single_quote(location).await.map(Some),
                    '"' => self.double_quote(location).await.map(Some),
                    _ => unreachable!(),
                }
            }
        }
    }

    // TODO Need more parameters to control how the word should be parsed. Especially:
    //  * Allow tilde expansion?
    /// Parses a word token.
    ///
    /// `is_delimiter` is a function that decides a character is a delimiter. The word ends when an
    /// unquoted delimiter is found. To parse a normal word token, you should pass
    /// [`is_token_delimiter_char`] as `is_delimiter`. Other functions can be passed to parse a
    /// word that ends with different delimiters.
    pub async fn word<F>(&mut self, mut is_delimiter: F) -> Result<Word>
    where
        F: FnMut(char) -> bool,
    {
        let location = self.location().await?.clone();
        let mut units = vec![];
        while let Some(unit) = self.word_unit(&mut is_delimiter).await? {
            units.push(unit)
        }
        Ok(Word { units, location })
    }

    /// Determines the token ID for the word.
    ///
    /// This is a helper function used by [`Lexer::token`] and does not support
    /// operators.
    async fn token_id(&mut self, word: &Word) -> Result<TokenId> {
        if word.units.is_empty() {
            return Ok(TokenId::EndOfInput);
        }

        if let Some(literal) = word.to_string_if_literal() {
            if let Ok(keyword) = Keyword::try_from(literal.as_str()) {
                return Ok(TokenId::Token(Some(keyword)));
            }

            if literal.chars().all(|c| c.is_ascii_digit()) {
                // TODO Do we need to handle line continuations?
                if let Some(next) = self.peek_char().await? {
                    if next.value == '<' || next.value == '>' {
                        return Ok(TokenId::IoNumber);
                    }
                }
            }
        }

        Ok(TokenId::Token(None))
    }

    /// Parses a token.
    ///
    /// If there is no more token that can be parsed, the result is a token with an empty word and
    /// [`EndOfInput`](TokenId::EndOfInput) token identifier.
    pub async fn token(&mut self) -> Result<Token> {
        if let Some(op) = self.operator().await? {
            return Ok(op);
        }

        let index = self.index();
        let word = self.word(is_token_delimiter_char).await?;
        let id = self.token_id(&word).await?;
        Ok(Token { word, id, index })
    }
}

// This is here to get better order of Lexer members in the doc.
mod heredoc;
pub mod keyword;

#[cfg(test)]
mod tests {
    use super::op::Operator;
    use super::*;
    use crate::alias::Alias;
    use crate::input::Context;
    use crate::input::Input;
    use crate::parser::core::ErrorCause;
    use crate::source::lines;
    use crate::source::Line;
    use crate::source::Source;
    use futures::executor::block_on;
    use std::fmt;
    use std::future::ready;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;

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

    #[test]
    fn lexer_line_continuations_success() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\\n");
        assert!(block_on(lexer.line_continuations()).is_ok());
        assert_eq!(block_on(lexer.peek_char()), Ok(None));

        let mut lexer = Lexer::with_source(Source::Unknown, "\\\n\\\n\\\n");
        assert!(block_on(lexer.line_continuations()).is_ok());
        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_line_continuations_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        assert!(block_on(lexer.line_continuations()).is_ok());
        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_line_continuations_not_backslash() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\n");
        assert!(block_on(lexer.line_continuations()).is_ok());

        let c = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.column.get(), 1);
    }

    #[test]
    fn lexer_line_continuations_only_backslash() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\");
        assert!(block_on(lexer.line_continuations()).is_ok());

        let c = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c.value, '\\');
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.column.get(), 1);
    }

    #[test]
    fn lexer_line_continuations_not_newline() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\\\");
        assert!(block_on(lexer.line_continuations()).is_ok());

        let c = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c.value, '\\');
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.column.get(), 1);
    }

    #[test]
    fn lexer_line_continuations_partial_match_after_success() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\\n\\\\");
        assert!(block_on(lexer.line_continuations()).is_ok());

        let c = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(c.value, '\\');
        assert_eq!(c.location.line.number.get(), 2);
        assert_eq!(c.location.column.get(), 1);
    }

    #[test]
    fn lexer_skip_blanks() {
        let mut lexer = Lexer::with_source(Source::Unknown, " \t w");

        let c = block_on(async {
            lexer.skip_blanks().await?;
            lexer.peek_char().await
        })
        .unwrap()
        .unwrap();
        assert_eq!(c.value, 'w');
        assert_eq!(c.location.line.value, " \t w");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 4);

        // Test idempotence
        let c = block_on(async {
            lexer.skip_blanks().await?;
            lexer.peek_char().await
        })
        .unwrap()
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
            let c1 = lexer.peek_char().await.unwrap().cloned();
            lexer.skip_blanks().await.unwrap();
            let c2 = lexer.peek_char().await.unwrap().cloned();
            (c1, c2)
        });
        assert_eq!(c1, c2);
    }

    #[test]
    fn lexer_skip_blanks_skips_line_continuations() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\\n  \\\n\\\n\\\n \\\nX");
        let c = block_on(async {
            lexer.skip_blanks().await?;
            lexer.peek_char().await
        })
        .unwrap()
        .unwrap();
        assert_eq!(c.value, 'X');
        assert_eq!(c.location.line.value, "X");
        assert_eq!(c.location.line.number.get(), 6);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 1);

        let mut lexer = Lexer::with_source(Source::Unknown, "  \\\n\\\n  \\\n Y");
        let c = block_on(async {
            lexer.skip_blanks().await?;
            lexer.peek_char().await
        })
        .unwrap()
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
            let c1 = lexer.peek_char().await.unwrap().cloned();
            lexer.skip_comment().await.unwrap();
            let c2 = lexer.peek_char().await.unwrap().cloned();
            (c1, c2)
        });
        assert_eq!(c1, c2);
    }

    #[test]
    fn lexer_skip_comment_empty_comment() {
        let mut lexer = Lexer::with_source(Source::Unknown, "#\n");

        let c = block_on(async {
            lexer.skip_comment().await?;
            lexer.peek_char().await
        })
        .unwrap()
        .unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.line.value, "#\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 2);

        // Test idempotence
        let c = block_on(async {
            lexer.skip_comment().await?;
            lexer.peek_char().await
        })
        .unwrap()
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
            lexer.skip_comment().await?;
            lexer.peek_char().await
        })
        .unwrap()
        .unwrap();
        assert_eq!(c.value, '\n');
        assert_eq!(c.location.line.value, "### foo bar\\\n");
        assert_eq!(c.location.line.number.get(), 1);
        assert_eq!(c.location.line.source, Source::Unknown);
        assert_eq!(c.location.column.get(), 13);

        // Test idempotence
        let c = block_on(async {
            lexer.skip_comment().await?;
            lexer.peek_char().await
        })
        .unwrap()
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

        let c = block_on(async {
            lexer.skip_comment().await?;
            lexer.peek_char().await
        });
        assert_eq!(c, Ok(None));

        // Test idempotence
        let c = block_on(async {
            lexer.skip_comment().await?;
            lexer.peek_char().await
        });
        assert_eq!(c, Ok(None));
    }

    #[test]
    fn lexer_operator_longest_match() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<-");

        let t = block_on(lexer.operator()).unwrap().unwrap();
        assert_eq!(t.word.units.len(), 3);
        assert_eq!(t.word.units[0], WordUnit::Unquoted(TextUnit::Literal('<')));
        assert_eq!(t.word.units[1], WordUnit::Unquoted(TextUnit::Literal('<')));
        assert_eq!(t.word.units[2], WordUnit::Unquoted(TextUnit::Literal('-')));
        assert_eq!(t.word.location.line.value, "<<-");
        assert_eq!(t.word.location.line.number.get(), 1);
        assert_eq!(t.word.location.line.source, Source::Unknown);
        assert_eq!(t.word.location.column.get(), 1);
        assert_eq!(t.id, TokenId::Operator(Operator::LessLessDash));

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_operator_delimited_by_another_operator() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<>");

        let t = block_on(lexer.operator()).unwrap().unwrap();
        assert_eq!(t.word.units.len(), 2);
        assert_eq!(t.word.units[0], WordUnit::Unquoted(TextUnit::Literal('<')));
        assert_eq!(t.word.units[1], WordUnit::Unquoted(TextUnit::Literal('<')));
        assert_eq!(t.word.location.line.value, "<<>");
        assert_eq!(t.word.location.line.number.get(), 1);
        assert_eq!(t.word.location.line.source, Source::Unknown);
        assert_eq!(t.word.location.column.get(), 1);
        assert_eq!(t.id, TokenId::Operator(Operator::LessLess));

        assert_eq!(block_on(lexer.location()).unwrap().column.get(), 3);
    }

    #[test]
    fn lexer_operator_delimited_by_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "<<");

        let t = block_on(lexer.operator()).unwrap().unwrap();
        assert_eq!(t.word.units.len(), 2);
        assert_eq!(t.word.units[0], WordUnit::Unquoted(TextUnit::Literal('<')));
        assert_eq!(t.word.units[1], WordUnit::Unquoted(TextUnit::Literal('<')));
        assert_eq!(t.word.location.line.value, "<<");
        assert_eq!(t.word.location.line.number.get(), 1);
        assert_eq!(t.word.location.line.source, Source::Unknown);
        assert_eq!(t.word.location.column.get(), 1);
        assert_eq!(t.id, TokenId::Operator(Operator::LessLess));

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_operator_containing_line_continuations() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\\n\\\n<\\\n<\\\n>");

        let t = block_on(lexer.operator()).unwrap().unwrap();
        assert_eq!(t.word.units.len(), 2);
        assert_eq!(t.word.units[0], WordUnit::Unquoted(TextUnit::Literal('<')));
        assert_eq!(t.word.units[1], WordUnit::Unquoted(TextUnit::Literal('<')));
        assert_eq!(t.word.location.line.value, "<\\\n");
        assert_eq!(t.word.location.line.number.get(), 3);
        assert_eq!(t.word.location.line.source, Source::Unknown);
        assert_eq!(t.word.location.column.get(), 1);
        assert_eq!(t.id, TokenId::Operator(Operator::LessLess));

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '>');
    }

    #[test]
    fn lexer_operator_none() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\\n ");

        let r = block_on(lexer.operator()).unwrap();
        assert!(r.is_none(), "Unexpected success: {:?}", r);
    }

    #[test]
    fn lexer_operator_should_not_peek_beyond_newline() {
        struct OneLineInput(Option<Line>);
        impl Input for OneLineInput {
            fn next_line(
                &mut self,
                _: &Context,
            ) -> Pin<Box<dyn Future<Output = crate::input::Result>>> {
                if let Some(line) = self.0.take() {
                    Box::pin(ready(Ok(line)))
                } else {
                    panic!("The second line should not be read")
                }
            }
        }

        let line = lines(Source::Unknown, "\n").next().unwrap();
        let mut lexer = Lexer::new(Box::new(OneLineInput(Some(line))));

        let t = block_on(lexer.operator()).unwrap().unwrap();
        assert_eq!(t.word.units, [WordUnit::Unquoted(TextUnit::Literal('\n'))]);
        assert_eq!(t.word.location.line.value, "\n");
        assert_eq!(t.word.location.line.number.get(), 1);
        assert_eq!(t.word.location.line.source, Source::Unknown);
        assert_eq!(t.word.location.column.get(), 1);
        assert_eq!(t.id, TokenId::Operator(Operator::Newline));
    }

    #[test]
    fn lexer_command_substitution_success() {
        let mut lexer = Lexer::with_source(Source::Unknown, "( foo bar )baz");
        let location = Location::dummy("X".to_string());

        let result = block_on(lexer.command_substitution(location))
            .unwrap()
            .unwrap();
        if let TextUnit::CommandSubst { location, content } = result {
            assert_eq!(location.line.value, "X");
            assert_eq!(location.line.number.get(), 1);
            assert_eq!(location.line.source, Source::Unknown);
            assert_eq!(location.column.get(), 1);
            assert_eq!(content, " foo bar ");
        } else {
            panic!("unexpected result {:?}", result);
        }

        let next = block_on(lexer.location()).unwrap();
        assert_eq!(next.line.value, "( foo bar )baz");
        assert_eq!(next.line.number.get(), 1);
        assert_eq!(next.line.source, Source::Unknown);
        assert_eq!(next.column.get(), 12);
    }

    #[test]
    fn lexer_command_substitution_none() {
        let mut lexer = Lexer::with_source(Source::Unknown, " foo bar )baz");
        let location = Location::dummy("Y".to_string());

        let result = block_on(lexer.command_substitution(location)).unwrap();
        assert_eq!(result, None);

        let next = block_on(lexer.location()).unwrap();
        assert_eq!(next.line.value, " foo bar )baz");
        assert_eq!(next.line.number.get(), 1);
        assert_eq!(next.line.source, Source::Unknown);
        assert_eq!(next.column.get(), 1);
    }

    #[test]
    fn lexer_command_substitution_unclosed() {
        let mut lexer = Lexer::with_source(Source::Unknown, "( foo bar baz");
        let location = Location::dummy("Z".to_string());

        let e = block_on(lexer.command_substitution(location)).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedCommandSubstitution { opening_location }) =
            e.cause
        {
            assert_eq!(opening_location.line.value, "Z");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("unexpected error cause {:?}", e);
        }
        assert_eq!(e.location.line.value, "( foo bar baz");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 14);
    }

    #[test]
    fn lexer_dollar_word_unit_no_dollar() {
        let mut lexer = Lexer::with_source(Source::Unknown, "foo");
        let result = block_on(lexer.dollar_word_unit()).unwrap();
        assert_eq!(result, None);

        let mut lexer = Lexer::with_source(Source::Unknown, "()");
        let result = block_on(lexer.dollar_word_unit()).unwrap();
        assert_eq!(result, None);
        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '(');

        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let result = block_on(lexer.dollar_word_unit()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn lexer_dollar_word_unit_dollar_followed_by_non_special() {
        let mut lexer = Lexer::with_source(Source::Unknown, "$;");
        let result = block_on(lexer.dollar_word_unit()).unwrap();
        assert_eq!(result, None);
        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '$');

        let mut lexer = Lexer::with_source(Source::Unknown, "$&");
        let result = block_on(lexer.dollar_word_unit()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn lexer_dollar_word_unit_command_substitution() {
        let mut lexer = Lexer::with_source(Source::Unknown, "$()");
        let result = block_on(lexer.dollar_word_unit()).unwrap().unwrap();
        if let TextUnit::CommandSubst { location, content } = result {
            assert_eq!(location.line.value, "$()");
            assert_eq!(location.line.number.get(), 1);
            assert_eq!(location.line.source, Source::Unknown);
            assert_eq!(location.column.get(), 1);
            assert_eq!(content, "");
        } else {
            panic!("unexpected result {:?}", result);
        }
        assert_eq!(block_on(lexer.peek_char()), Ok(None));

        let mut lexer = Lexer::with_source(Source::Unknown, "$( foo bar )");
        let result = block_on(lexer.dollar_word_unit()).unwrap().unwrap();
        if let TextUnit::CommandSubst { location, content } = result {
            assert_eq!(location.line.value, "$( foo bar )");
            assert_eq!(location.line.number.get(), 1);
            assert_eq!(location.line.source, Source::Unknown);
            assert_eq!(location.column.get(), 1);
            assert_eq!(content, " foo bar ");
        } else {
            panic!("unexpected result {:?}", result);
        }
        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_text_unit_literal_accepted() {
        let mut lexer = Lexer::with_source(Source::Unknown, "X");
        let mut called = false;
        let result = block_on(lexer.text_unit(
            |c| {
                called = true;
                assert_eq!(c, 'X');
                false
            },
            |c| panic!("unexpected call to is_escapable({:?})", c),
        ))
        .unwrap()
        .unwrap();
        assert!(called);
        if let Literal(c) = result {
            assert_eq!(c, 'X');
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_text_unit_literal_rejected() {
        let mut lexer = Lexer::with_source(Source::Unknown, ";");
        let mut called = false;
        let result = block_on(lexer.text_unit(
            |c| {
                called = true;
                assert_eq!(c, ';');
                true
            },
            |c| panic!("unexpected call to is_escapable({:?})", c),
        ))
        .unwrap();
        assert!(called);
        assert_eq!(result, None);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, ';');
    }

    #[test]
    fn lexer_text_unit_backslash_accepted() {
        let mut lexer = Lexer::with_source(Source::Unknown, r"\#");
        let mut called = false;
        let result = block_on(lexer.text_unit(
            |c| panic!("unexpected call to is_delimiter({:?})", c),
            |c| {
                called = true;
                assert_eq!(c, '#');
                true
            },
        ))
        .unwrap()
        .unwrap();
        assert!(called);
        assert_eq!(result, Backslashed('#'));

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_text_unit_backslash_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, r"\");
        let result = block_on(lexer.text_unit(
            |c| panic!("unexpected call to is_delimiter({:?})", c),
            |c| panic!("unexpected call to is_escapable({:?})", c),
        ))
        .unwrap()
        .unwrap();
        assert_eq!(result, Literal('\\'));

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_text_unit_dollar() {
        let mut lexer = Lexer::with_source(Source::Unknown, "$()");
        let result = block_on(lexer.text_unit(
            |c| panic!("unexpected call to is_delimiter({:?})", c),
            |c| panic!("unexpected call to is_escapable({:?})", c),
        ))
        .unwrap()
        .unwrap();
        if let CommandSubst { content, location } = result {
            assert_eq!(content, "");
            assert_eq!(location.column.get(), 1);
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_text_unit_line_continuations() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\\\n\\\nX");
        let result = block_on(lexer.text_unit(
            |_| false,
            |c| panic!("unexpected call to is_escapable({:?})", c),
        ))
        .unwrap()
        .unwrap();
        if let Literal(c) = result {
            assert_eq!(c, 'X');
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_text_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let Text(units) = block_on(lexer.text(
            |c| panic!("unexpected call to is_delimiter({:?})", c),
            |c| panic!("unexpected call to is_escapable({:?})", c),
        ))
        .unwrap();
        assert_eq!(units, &[]);
    }

    #[test]
    fn lexer_text_nonempty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "abc");
        let mut called = 0;
        let Text(units) = block_on(lexer.text(
            |c| {
                assert!(
                    matches!(c, 'a' | 'b' | 'c'),
                    "unexpected call to is_delimiter({:?}), called={}",
                    c,
                    called
                );
                called += 1;
                false
            },
            |c| panic!("unexpected call to is_escapable({:?})", c),
        ))
        .unwrap();
        assert_eq!(units, &[Literal('a'), Literal('b'), Literal('c')]);
        assert_eq!(called, 3);

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_text_delimiter() {
        let mut lexer = Lexer::with_source(Source::Unknown, "abc");
        let mut called = 0;
        let Text(units) = block_on(lexer.text(
            |c| {
                assert!(
                    matches!(c, 'a' | 'b' | 'c'),
                    "unexpected call to is_delimiter({:?}), called={}",
                    c,
                    called
                );
                called += 1;
                c == 'c'
            },
            |c| panic!("unexpected call to is_escapable({:?})", c),
        ))
        .unwrap();
        assert_eq!(units, &[Literal('a'), Literal('b')]);
        assert_eq!(called, 3);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, 'c');
    }

    #[test]
    fn lexer_text_escaping() {
        let mut lexer = Lexer::with_source(Source::Unknown, r"a\b\c");
        let mut called = 0;
        let Text(units) = block_on(lexer.text(
            |_| false,
            |c| {
                assert!(
                    matches!(c, 'b' | 'c'),
                    "unexpected call to is_escapable({:?}), called={}",
                    c,
                    called
                );
                called += 1;
                c == 'b'
            },
        ))
        .unwrap();
        assert_eq!(
            units,
            &[Literal('a'), Backslashed('b'), Literal('\\'), Literal('c')]
        );
        assert_eq!(called, 2);

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_unquoted() {
        let mut lexer = Lexer::with_source(Source::Unknown, "$()");
        let result =
            block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
                .unwrap()
                .unwrap();
        if let Unquoted(CommandSubst { content, location }) = result {
            assert_eq!(content, "");
            assert_eq!(location.column.get(), 1);
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_unquoted_escapes() {
        // Any characters can be escaped in this context.
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, r#"\a\$\`\"\\\'\#"#);
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('a')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('$')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('`')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('"')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('\\')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('\'')))));
            let result = lexer
                .word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c))
                .await;
            assert_eq!(result, Ok(Some(Unquoted(Backslashed('#')))));

            assert_eq!(lexer.peek_char().await, Ok(None));
        })
    }

    #[test]
    fn lexer_word_unit_single_quote_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "''");
        let result =
            block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
                .unwrap()
                .unwrap();
        if let SingleQuote(content) = result {
            assert_eq!(content, "");
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_single_quote_nonempty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "'abc\n$def\\'");
        let result =
            block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
                .unwrap()
                .unwrap();
        if let SingleQuote(content) = result {
            assert_eq!(content, "abc\n$def\\");
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_single_quote_unclosed() {
        let mut lexer = Lexer::with_source(Source::Unknown, "'abc\ndef\\");

        let e = block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
            .unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedSingleQuote { opening_location }) = e.cause {
            assert_eq!(opening_location.line.value, "'abc\n");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("unexpected error cause {:?}", e);
        }
        assert_eq!(e.location.line.value, "def\\");
        assert_eq!(e.location.line.number.get(), 2);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 5);
    }

    #[test]
    fn lexer_word_unit_double_quote_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\"\"");
        let result =
            block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
                .unwrap()
                .unwrap();
        if let DoubleQuote(Text(content)) = result {
            assert_eq!(content, []);
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_double_quote_non_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\"abc\"");
        let result =
            block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
                .unwrap()
                .unwrap();
        if let DoubleQuote(Text(content)) = result {
            assert_eq!(content, [Literal('a'), Literal('b'), Literal('c')]);
        } else {
            panic!("unexpected result {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_unit_double_quote_escapes() {
        // Only the following can be escaped in this context: $ ` " \
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, r#""\a\$\`\"\\\'\#""#);
            let result = lexer
                .word_unit(|c| match c {
                    'a' | '\'' | '#' => true,
                    _ => panic!("unexpected call to is_delimiter({:?})", c),
                })
                .await
                .unwrap()
                .unwrap();
            if let DoubleQuote(Text(ref units)) = result {
                assert_eq!(
                    units,
                    &[
                        Literal('\\'),
                        Literal('a'),
                        Backslashed('$'),
                        Backslashed('`'),
                        Backslashed('"'),
                        Backslashed('\\'),
                        Literal('\\'),
                        Literal('\''),
                        Literal('\\'),
                        Literal('#'),
                    ]
                );
            } else {
                panic!("Not a double quote: {:?}", result);
            }

            assert_eq!(lexer.peek_char().await, Ok(None));
        })
    }

    #[test]
    fn lexer_word_unit_double_quote_unclosed() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\"abc\ndef");

        let e = block_on(lexer.word_unit(|c| panic!("unexpected call to is_delimiter({:?})", c)))
            .unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedDoubleQuote { opening_location }) = e.cause {
            assert_eq!(opening_location.line.value, "\"abc\n");
            assert_eq!(opening_location.line.number.get(), 1);
            assert_eq!(opening_location.line.source, Source::Unknown);
            assert_eq!(opening_location.column.get(), 1);
        } else {
            panic!("unexpected error cause {:?}", e);
        }
        assert_eq!(e.location.line.value, "def");
        assert_eq!(e.location.line.number.get(), 2);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 4);
    }

    #[test]
    fn lexer_word_nonempty() {
        let mut lexer = Lexer::with_source(Source::Unknown, r"0$(:)X\#");
        let word = block_on(lexer.word(|_| false)).unwrap();
        assert_eq!(word.units.len(), 4);
        assert_eq!(word.units[0], WordUnit::Unquoted(TextUnit::Literal('0')));
        if let WordUnit::Unquoted(TextUnit::CommandSubst { content, location }) = &word.units[1] {
            assert_eq!(content, ":");
            assert_eq!(location.line.value, r"0$(:)X\#");
            assert_eq!(location.line.number.get(), 1);
            assert_eq!(location.line.source, Source::Unknown);
            assert_eq!(location.column.get(), 2);
        } else {
            panic!("unexpected word unit: {:?}", word.units[1]);
        }
        assert_eq!(word.units[2], WordUnit::Unquoted(TextUnit::Literal('X')));
        assert_eq!(
            word.units[3],
            WordUnit::Unquoted(TextUnit::Backslashed('#'))
        );
        assert_eq!(word.location.line.value, r"0$(:)X\#");
        assert_eq!(word.location.line.number.get(), 1);
        assert_eq!(word.location.line.source, Source::Unknown);
        assert_eq!(word.location.column.get(), 1);

        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_word_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let word = block_on(lexer.word(|_| panic!("unexpected call to is_delimiter"))).unwrap();
        assert_eq!(word.units, []);
        assert_eq!(word.location.line.value, "");
        assert_eq!(word.location.line.number.get(), 1);
        assert_eq!(word.location.line.source, Source::Unknown);
        assert_eq!(word.location.column.get(), 1);
    }

    #[test]
    fn lexer_token_empty() {
        // If there's no word unit that can be parsed, it is the end of input.
        let mut lexer = Lexer::with_source(Source::Unknown, "");

        let t = block_on(lexer.token()).unwrap();
        assert_eq!(t.word.location.line.value, "");
        assert_eq!(t.word.location.line.number.get(), 1);
        assert_eq!(t.word.location.line.source, Source::Unknown);
        assert_eq!(t.word.location.column.get(), 1);
        assert_eq!(t.id, TokenId::EndOfInput);
        assert_eq!(t.index, 0);
    }

    #[test]
    fn lexer_token_non_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "abc ");

        let t = block_on(lexer.token()).unwrap();
        assert_eq!(t.word.units.len(), 3);
        assert_eq!(t.word.units[0], WordUnit::Unquoted(TextUnit::Literal('a')));
        assert_eq!(t.word.units[1], WordUnit::Unquoted(TextUnit::Literal('b')));
        assert_eq!(t.word.units[2], WordUnit::Unquoted(TextUnit::Literal('c')));
        assert_eq!(t.word.location.line.value, "abc ");
        assert_eq!(t.word.location.line.number.get(), 1);
        assert_eq!(t.word.location.line.source, Source::Unknown);
        assert_eq!(t.word.location.column.get(), 1);
        assert_eq!(t.id, TokenId::Token(None));
        assert_eq!(t.index, 0);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, ' ');
    }

    #[test]
    fn lexer_token_io_number_delimited_by_less() {
        let mut lexer = Lexer::with_source(Source::Unknown, "12<");

        let t = block_on(lexer.token()).unwrap();
        assert_eq!(t.word.units.len(), 2);
        assert_eq!(t.word.units[0], WordUnit::Unquoted(TextUnit::Literal('1')));
        assert_eq!(t.word.units[1], WordUnit::Unquoted(TextUnit::Literal('2')));
        assert_eq!(t.word.location.line.value, "12<");
        assert_eq!(t.word.location.line.number.get(), 1);
        assert_eq!(t.word.location.line.source, Source::Unknown);
        assert_eq!(t.word.location.column.get(), 1);
        assert_eq!(t.id, TokenId::IoNumber);
        assert_eq!(t.index, 0);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '<');
    }

    #[test]
    fn lexer_token_io_number_delimited_by_greater() {
        let mut lexer = Lexer::with_source(Source::Unknown, "0>>");

        let t = block_on(lexer.token()).unwrap();
        assert_eq!(t.word.units.len(), 1);
        assert_eq!(t.word.units[0], WordUnit::Unquoted(TextUnit::Literal('0')));
        assert_eq!(t.word.location.line.value, "0>>");
        assert_eq!(t.word.location.line.number.get(), 1);
        assert_eq!(t.word.location.line.source, Source::Unknown);
        assert_eq!(t.word.location.column.get(), 1);
        assert_eq!(t.id, TokenId::IoNumber);
        assert_eq!(t.index, 0);

        assert_eq!(block_on(lexer.location()).unwrap().column.get(), 2);
    }

    #[test]
    fn lexer_token_after_blank() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, " a  ");

            lexer.skip_blanks().await.unwrap();
            let t = lexer.token().await.unwrap();
            assert_eq!(t.word.location.line.value, " a  ");
            assert_eq!(t.word.location.line.number.get(), 1);
            assert_eq!(t.word.location.line.source, Source::Unknown);
            assert_eq!(t.word.location.column.get(), 2);
            assert_eq!(t.id, TokenId::Token(None));
            assert_eq!(t.index, 1);

            lexer.skip_blanks().await.unwrap();
            let t = lexer.token().await.unwrap();
            assert_eq!(t.word.location.line.value, " a  ");
            assert_eq!(t.word.location.line.number.get(), 1);
            assert_eq!(t.word.location.line.source, Source::Unknown);
            assert_eq!(t.word.location.column.get(), 5);
            assert_eq!(t.id, TokenId::EndOfInput);
            assert_eq!(t.index, 4);
        });
    }
}
