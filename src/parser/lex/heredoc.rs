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
use crate::syntax::HereDoc;
use crate::syntax::Text;
use crate::syntax::TextUnit::Literal;
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

impl Lexer {
    /// Reads a line literally.
    ///
    /// This function recognizes no quotes or expansions. Starting from the
    /// current position, the line is read up to (but not including) the
    /// terminating newline.
    pub async fn line(&mut self) -> Result<String> {
        let mut line = String::new();
        while let Some(c) = self.consume_char_if(|c| c != NEWLINE).await? {
            line.push(c.value);
        }
        Ok(line)
    }

    /// Parses the content of a here-document.
    pub async fn here_doc_content(&mut self, heredoc: PartialHereDoc) -> Result<HereDoc> {
        fn is_escapable(c: char) -> bool {
            matches!(c, '$' | '`' | '\\')
        }

        let delimiter = heredoc.delimiter;
        let remove_tabs = heredoc.remove_tabs;

        let (delimiter_string, literal) = delimiter.unquote();
        // TODO Reject if the delimiter contains a newline
        let mut content = Text(vec![]);
        loop {
            let (line_text, line_string) = if literal {
                let line_string = self.line().await?;
                let line_text = Text::from_literal_chars(line_string.chars());
                (line_text, line_string)
            } else {
                let line_text = self.text(|c| c == NEWLINE, is_escapable).await?;
                let line_string = line_text.to_string();
                (line_text, line_string)
            };
            // TODO Strip leading tabs depending on the here-doc operator type

            if !self.skip_if(|c| c == NEWLINE).await? {
                todo!("Return an error: unexpected EOF, the delimiter missing");
            }

            if line_string == delimiter_string {
                return Ok(HereDoc {
                    delimiter,
                    remove_tabs,
                    content,
                });
            }

            content.0.extend(line_text.0);
            content.0.push(Literal(NEWLINE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use crate::syntax::TextUnit::*;
    use futures::executor::block_on;

    #[test]
    fn lexer_line() {
        let mut lexer = Lexer::with_source(Source::Unknown, "\n");
        let line = block_on(lexer.line()).unwrap();
        assert_eq!(line, "");

        let mut lexer = Lexer::with_source(Source::Unknown, "foo\n");
        let line = block_on(lexer.line()).unwrap();
        assert_eq!(line, "foo");
        let next = block_on(lexer.peek_char()).unwrap().unwrap();
        assert_eq!(next.value, '\n');
    }

    fn partial_here_doc(delimiter: &str, remove_tabs: bool) -> PartialHereDoc {
        PartialHereDoc {
            delimiter: delimiter.parse().unwrap(),
            remove_tabs,
        }
    }

    #[test]
    fn lexer_here_doc_content_empty_content() {
        let heredoc = partial_here_doc("END", false);

        let mut lexer = Lexer::with_source(Source::Unknown, "END\nX");
        let heredoc = block_on(lexer.here_doc_content(heredoc)).unwrap();
        assert_eq!(heredoc.delimiter.to_string(), "END");
        assert_eq!(heredoc.remove_tabs, false);
        assert_eq!(heredoc.content.0, []);

        let location = block_on(lexer.location()).unwrap();
        assert_eq!(location.line.number.get(), 2);
        assert_eq!(location.column.get(), 1);
    }

    #[test]
    fn lexer_here_doc_content_one_line_content() {
        let heredoc = partial_here_doc("FOO", false);

        let mut lexer = Lexer::with_source(Source::Unknown, "content\nFOO\nX");
        let heredoc = block_on(lexer.here_doc_content(heredoc)).unwrap();
        assert_eq!(heredoc.delimiter.to_string(), "FOO");
        assert_eq!(heredoc.remove_tabs, false);
        assert_eq!(heredoc.content.to_string(), "content\n");

        let location = block_on(lexer.location()).unwrap();
        assert_eq!(location.line.number.get(), 3);
        assert_eq!(location.column.get(), 1);
    }

    #[test]
    fn lexer_here_doc_content_long_content() {
        let heredoc = partial_here_doc("BAR", false);

        let mut lexer = Lexer::with_source(Source::Unknown, "foo\n\tBAR\n\nbaz\nBAR\nX");
        let heredoc = block_on(lexer.here_doc_content(heredoc)).unwrap();
        assert_eq!(heredoc.delimiter.to_string(), "BAR");
        assert_eq!(heredoc.remove_tabs, false);
        assert_eq!(heredoc.content.to_string(), "foo\n\tBAR\n\nbaz\n");

        let location = block_on(lexer.location()).unwrap();
        assert_eq!(location.line.number.get(), 6);
        assert_eq!(location.column.get(), 1);
    }

    #[test]
    fn lexer_here_doc_content_escapes_with_unquoted_delimiter() {
        let heredoc = partial_here_doc("END", false);

        let mut lexer = Lexer::with_source(
            Source::Unknown,
            r#"\a\$\"\'\`\\\
X
END
"#,
        );
        let heredoc = block_on(lexer.here_doc_content(heredoc)).unwrap();
        assert_eq!(
            heredoc.content.0,
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
        let heredoc = partial_here_doc(r"\END", false);

        let mut lexer = Lexer::with_source(
            Source::Unknown,
            r#"\a\$\"\'\`\\\
X
END
"#,
        );
        let heredoc = block_on(lexer.here_doc_content(heredoc)).unwrap();
        assert_eq!(
            heredoc.content.0,
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
}
