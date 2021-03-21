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
    /// Parses the content of a here-document.
    pub async fn here_doc_content(&mut self, heredoc: PartialHereDoc) -> Result<HereDoc> {
        let delimiter = heredoc.delimiter;
        let remove_tabs = heredoc.remove_tabs;

        // TODO Unquote the delimiter string
        let delimiter_string = delimiter.to_string();
        // TODO Reject if the delimiter contains a newline
        let mut content = Text(vec![]);
        loop {
            // TODO If the delimiter is not quoted, backslashes should be effective only before
            // expansions and newlines
            // TODO If the delimiter is quoted, the here-doc content should be literal.
            let line = self.text(|c| c == NEWLINE, |_| false).await?;
            // TODO Strip leading tabs depending on the here-doc operator type
            let line_string = line.to_string();

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

            content.0.extend(line.0);
            content.0.push(Literal(NEWLINE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use futures::executor::block_on;

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
}
