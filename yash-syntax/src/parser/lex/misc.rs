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
        F: FnMut(char) -> bool,
    {
        Ok(self.consume_char_if(f).await?.is_some())
    }

    /// Skips blank characters until reaching a non-blank.
    pub async fn skip_blanks(&mut self) -> Result<()> {
        while self.skip_if(is_blank).await? {}
        Ok(())
    }

    /// Skips a comment, if any.
    ///
    /// A comment ends just before a newline. The newline is *not* part of the comment.
    ///
    /// This function does not recognize line continuation inside the comment.
    pub async fn skip_comment(&mut self) -> Result<()> {
        if self.skip_if(|c| c == '#').await? {
            let mut lexer = self.disable_line_continuation();
            while lexer.skip_if(|c| c != '\n').await? {}
            Lexer::enable_line_continuation(lexer);
        }
        Ok(())
    }

    /// Skips blank characters and a comment, if any.
    ///
    /// This function is the same as [`skip_blanks`](Lexer::skip_blanks)
    /// followed by [`skip_comment`](Lexer::skip_comment).
    pub async fn skip_blanks_and_comment(&mut self) -> Result<()> {
        self.skip_blanks().await?;
        self.skip_comment().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use futures_executor::block_on;

    #[test]
    fn lexer_skip_blanks() {
        let mut lexer = Lexer::from_memory(" \t w", Source::Unknown);

        let c = block_on(async {
            lexer.skip_blanks().await?;
            lexer.peek_char().await
        });
        assert_eq!(c, Ok(Some('w')));

        // Test idempotence
        let c = block_on(async {
            lexer.skip_blanks().await?;
            lexer.peek_char().await
        });
        assert_eq!(c, Ok(Some('w')));
    }

    #[test]
    fn lexer_skip_blanks_does_not_skip_newline() {
        let mut lexer = Lexer::from_memory("\n", Source::Unknown);
        block_on(async {
            lexer.skip_blanks().await.unwrap();
            assert_eq!(lexer.peek_char().await, Ok(Some('\n')));
        });
    }

    #[test]
    fn lexer_skip_blanks_skips_line_continuations() {
        let mut lexer = Lexer::from_memory("\\\n  \\\n\\\n\\\n \\\nX", Source::Unknown);
        let c = block_on(async {
            lexer.skip_blanks().await?;
            lexer.peek_char().await
        });
        assert_eq!(c, Ok(Some('X')));

        let mut lexer = Lexer::from_memory("  \\\n\\\n  \\\n Y", Source::Unknown);
        let c = block_on(async {
            lexer.skip_blanks().await?;
            lexer.peek_char().await
        });
        assert_eq!(c, Ok(Some('Y')));
    }

    #[test]
    fn lexer_skip_comment_no_comment() {
        let mut lexer = Lexer::from_memory("\n", Source::Unknown);
        block_on(async {
            lexer.skip_comment().await.unwrap();
            assert_eq!(lexer.peek_char().await, Ok(Some('\n')));
        });
    }

    #[test]
    fn lexer_skip_comment_empty_comment() {
        let mut lexer = Lexer::from_memory("#\n", Source::Unknown);

        let c = block_on(async {
            lexer.skip_comment().await?;
            lexer.peek_char().await
        });
        assert_eq!(c, Ok(Some('\n')));

        // Test idempotence
        let c = block_on(async {
            lexer.skip_comment().await?;
            lexer.peek_char().await
        });
        assert_eq!(c, Ok(Some('\n')));
    }

    #[test]
    fn lexer_skip_comment_non_empty_comment() {
        let mut lexer = Lexer::from_memory("\\\n### foo bar\\\n", Source::Unknown);

        let c = block_on(async {
            lexer.skip_comment().await?;
            lexer.peek_char().await
        });
        assert_eq!(c, Ok(Some('\n')));
        assert_eq!(lexer.index(), 14);

        // Test idempotence
        let c = block_on(async {
            lexer.skip_comment().await?;
            lexer.peek_char().await
        });
        assert_eq!(c, Ok(Some('\n')));
        assert_eq!(lexer.index(), 14);
    }

    #[test]
    fn lexer_skip_comment_not_ending_with_newline() {
        let mut lexer = Lexer::from_memory("#comment", Source::Unknown);

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
