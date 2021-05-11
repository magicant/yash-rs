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

//! Extension of the core for implementing the rest of the lexer.

use super::core::is_blank;
use super::core::Lexer;
use crate::parser::core::Result;

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
        self.many(line_continuation).await.map(drop)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use futures::executor::block_on;

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
}
