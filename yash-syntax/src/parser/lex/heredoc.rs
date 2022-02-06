// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Here-document content parser

use super::Lexer;
use crate::parser::core::Result;
use crate::parser::error::Error;
use crate::parser::error::SyntaxError;
use crate::syntax::HereDoc;
use crate::syntax::Text;
use crate::syntax::TextUnit::{self, Literal};
use crate::syntax::Unquote;
use crate::syntax::Word;

/// Here-document without a content.
///
/// This is a temporary data that is created when a here-document operator is parsed. It contains
/// the delimiter word and the type of the operator. It is used later when the content of the
/// here-document is read.
#[derive(Debug)]
pub struct PartialHereDoc {
    /// Token that marks the end of the content of the here-document.
    pub delimiter: Word,

    /// Whether leading tab characters should be removed from each line of the
    /// here-document content. This value is `true` for the `<<-` operator and
    /// `false` for `<<`.
    pub remove_tabs: bool,
}

const NEWLINE: char = '\n';

/// Counts the number of leading literal tab characters in `i`.
fn leading_tabs<'a, I: IntoIterator<Item = &'a TextUnit>>(i: I) -> usize {
    i.into_iter()
        .take_while(|&unit| unit == &Literal('\t'))
        .count()
}

impl Lexer<'_> {
    /// Reads a line literally.
    ///
    /// This function recognizes no quotes or expansions. Starting from the
    /// current position, the line is read up to (but not including) the
    /// terminating newline.
    pub async fn line(&mut self) -> Result<String> {
        let mut line = String::new();
        let mut lexer = self.disable_line_continuation();
        while let Some(c) = lexer.consume_char_if(|c| c != NEWLINE).await? {
            line.push(c.value);
        }
        Lexer::enable_line_continuation(lexer);
        Ok(line)
    }

    /// Parses the content of a here-document.
    ///
    /// This function reads here-document content corresponding to the
    /// here-document operator represented by the argument and fills
    /// `here_doc.content` with the results. The argument does not have to be
    /// mutable because `here_doc.content` is a `RefCell`. Note that this
    /// function will panic if `here_doc.content` has been borrowed.
    ///
    /// In case of an error, partial results may be left in `here_doc.content`.
    pub async fn here_doc_content(&mut self, here_doc: &HereDoc) -> Result<()> {
        fn is_escapable(c: char) -> bool {
            matches!(c, '$' | '`' | '\\')
        }

        let (delimiter_string, literal) = here_doc.delimiter.unquote();
        // TODO Reject if the delimiter contains a newline
        let mut content = here_doc.content.borrow_mut();
        loop {
            let (line_text, line_string) = if literal {
                let line_string = self.line().await?;
                let line_text = Text::from_literal_chars(line_string.chars());
                (line_text, line_string)
            } else {
                let begin = self.index();
                let line_text = self.text(|c| c == NEWLINE, is_escapable).await?;
                let end = self.index();
                let line_string = self.source_string(begin..end);
                (line_text, line_string)
            };

            if !self.skip_if(|c| c == NEWLINE).await? {
                let redir_op_location = here_doc.delimiter.location.clone();
                let cause = SyntaxError::UnclosedHereDocContent { redir_op_location }.into();
                let location = self.location().await?.clone();
                return Err(Error { cause, location });
            }

            let skip_count = if here_doc.remove_tabs {
                leading_tabs(&line_text.0)
            } else {
                0
            };
            if line_string[skip_count..] == delimiter_string {
                return Ok(());
            }

            content.0.extend({ line_text }.0.drain(skip_count..));
            content.0.push(Literal(NEWLINE));
        }
    }
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorCause;
    use crate::source::Source;
    use crate::syntax::TextUnit::*;
    use assert_matches::assert_matches;
    use futures_executor::block_on;
    use std::cell::RefCell;

    #[test]
    fn leading_tabs_test() {
        let c = leading_tabs(std::iter::empty());
        assert_eq!(c, 0);
        let c = leading_tabs(&[Literal('\t'), Literal('a')]);
        assert_eq!(c, 1);
        let c = leading_tabs(&[Literal('\t'), Literal('\t'), Literal('\t')]);
        assert_eq!(c, 3);
    }

    #[test]
    fn lexer_line() {
        let mut lexer = Lexer::from_memory("\n", Source::Unknown);
        let line = block_on(lexer.line()).unwrap();
        assert_eq!(line, "");

        let mut lexer = Lexer::from_memory("foo\n", Source::Unknown);
        let line = block_on(lexer.line()).unwrap();
        assert_eq!(line, "foo");
        let next = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(next, '\n');
    }

    fn here_doc_operator(delimiter: &str, remove_tabs: bool) -> HereDoc {
        HereDoc {
            delimiter: delimiter.parse().unwrap(),
            remove_tabs,
            content: RefCell::new(Text(Vec::new())),
        }
    }

    #[test]
    fn lexer_here_doc_content_empty_content() {
        let heredoc = here_doc_operator("END", false);

        let mut lexer = Lexer::from_memory("END\nX", Source::Unknown);
        block_on(lexer.here_doc_content(&heredoc)).unwrap();
        assert_eq!(heredoc.delimiter.to_string(), "END");
        assert_eq!(heredoc.remove_tabs, false);
        assert_eq!(heredoc.content.borrow().0, []);

        let location = block_on(lexer.location()).unwrap();
        assert_eq!(*location.code.value.borrow(), "END\nX");
        assert_eq!(location.code.start_line_number.get(), 1);
        assert_eq!(location.index, 4);
    }

    #[test]
    fn lexer_here_doc_content_one_line_content() {
        let heredoc = here_doc_operator("FOO", false);

        let mut lexer = Lexer::from_memory("content\nFOO\nX", Source::Unknown);
        block_on(lexer.here_doc_content(&heredoc)).unwrap();
        assert_eq!(heredoc.delimiter.to_string(), "FOO");
        assert_eq!(heredoc.remove_tabs, false);
        assert_eq!(heredoc.content.borrow().to_string(), "content\n");

        let location = block_on(lexer.location()).unwrap();
        assert_eq!(*location.code.value.borrow(), "content\nFOO\nX");
        assert_eq!(location.code.start_line_number.get(), 1);
        assert_eq!(location.index, 12);
    }

    #[test]
    fn lexer_here_doc_content_long_content() {
        let heredoc = here_doc_operator("BAR", false);

        let mut lexer = Lexer::from_memory("foo\n\tBAR\n\nbaz\nBAR\nX", Source::Unknown);
        block_on(lexer.here_doc_content(&heredoc)).unwrap();
        assert_eq!(heredoc.delimiter.to_string(), "BAR");
        assert_eq!(heredoc.remove_tabs, false);
        assert_eq!(heredoc.content.borrow().to_string(), "foo\n\tBAR\n\nbaz\n");

        let location = block_on(lexer.location()).unwrap();
        assert_eq!(*location.code.value.borrow(), "foo\n\tBAR\n\nbaz\nBAR\nX");
        assert_eq!(location.code.start_line_number.get(), 1);
        assert_eq!(location.index, 18);
    }

    #[test]
    fn lexer_here_doc_content_escapes_with_unquoted_delimiter() {
        let heredoc = here_doc_operator("END", false);

        let mut lexer = Lexer::from_memory(
            r#"\a\$\"\'\`\\\
X
END
"#,
            Source::Unknown,
        );
        block_on(lexer.here_doc_content(&heredoc)).unwrap();
        assert_eq!(
            heredoc.content.borrow().0,
            [
                Literal('\\'),
                Literal('a'),
                Backslashed('$'),
                Literal('\\'),
                Literal('"'),
                Literal('\\'),
                Literal('\''),
                Backslashed('`'),
                Backslashed('\\'),
                Literal('X'),
                Literal('\n'),
            ]
        );
    }

    #[test]
    fn lexer_here_doc_content_escapes_with_quoted_delimiter() {
        let heredoc = here_doc_operator(r"\END", false);

        let mut lexer = Lexer::from_memory(
            r#"\a\$\"\'\`\\\
X
END
"#,
            Source::Unknown,
        );
        block_on(lexer.here_doc_content(&heredoc)).unwrap();
        assert_eq!(
            heredoc.content.borrow().0,
            [
                Literal('\\'),
                Literal('a'),
                Literal('\\'),
                Literal('$'),
                Literal('\\'),
                Literal('"'),
                Literal('\\'),
                Literal('\''),
                Literal('\\'),
                Literal('`'),
                Literal('\\'),
                Literal('\\'),
                Literal('\\'),
                Literal('\n'),
                Literal('X'),
                Literal('\n'),
            ]
        );
    }

    #[test]
    fn lexer_here_doc_content_with_tabs_removed() {
        let heredoc = here_doc_operator("BAR", true);

        let mut lexer = Lexer::from_memory("\t\t\tfoo\n\tBAR\n\nbaz\nBAR\nX", Source::Unknown);
        block_on(lexer.here_doc_content(&heredoc)).unwrap();
        assert_eq!(heredoc.delimiter.to_string(), "BAR");
        assert_eq!(heredoc.remove_tabs, true);
        assert_eq!(heredoc.content.borrow().to_string(), "foo\n");

        let location = block_on(lexer.location()).unwrap();
        assert_eq!(*location.code.value.borrow(), "\t\t\tfoo\n\tBAR\n\n");
        assert_eq!(location.code.start_line_number.get(), 1);
        assert_eq!(location.index, 12);
    }

    #[test]
    fn lexer_here_doc_content_unclosed() {
        let heredoc = here_doc_operator("END", false);

        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let e = block_on(lexer.here_doc_content(&heredoc)).unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedHereDocContent { redir_op_location }) => {
            assert_eq!(*redir_op_location.code.value.borrow(), "END");
            assert_eq!(redir_op_location.code.start_line_number.get(), 1);
            assert_eq!(redir_op_location.code.source, Source::Unknown);
            assert_eq!(redir_op_location.index, 0);
        });
        assert_eq!(*e.location.code.value.borrow(), "");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.index, 0);
    }
}
