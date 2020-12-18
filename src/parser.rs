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

mod core;
mod fill;

use super::source::*;
use super::syntax::*;

pub use self::core::AsyncFnMut;
pub use self::core::AsyncFnOnce;
pub use self::core::Error;
pub use self::core::ErrorCause;
pub use self::core::Result;
pub use self::fill::Fill;
pub use self::fill::MissingHereDoc;

pub use self::core::Lexer;

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
}

pub use self::core::Parser;

impl Parser<'_> {
    /// Parses a simple command.
    pub async fn simple_command(&mut self) -> Result<SimpleCommand<MissingHereDoc>> {
        // TODO Support assignments and redirections. Stop on a delimiter token.
        let mut words = vec![];
        loop {
            let word = self.take_token().await;
            if let Err(Error {
                cause: ErrorCause::EndOfInput,
                ..
            }) = word
            {
                break;
            }
            words.push(word?);
        }
        Ok(SimpleCommand {
            words,
            redirs: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

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
}
