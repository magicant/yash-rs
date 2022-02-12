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

//! Part of the lexer that parses dollar units.
//!
//! Note that the detail lexer for each type of dollar units in another
//! dedicated module.

use super::core::WordLexer;
use crate::parser::core::Result;
use crate::syntax::TextUnit;

impl WordLexer<'_, '_> {
    /// Parses a text unit that starts with `$`.
    ///
    /// If the next character is `$`, a parameter expansion, command
    /// substitution, or arithmetic expansion is parsed. Otherwise, no
    /// characters are consumed and the return value is `Ok(None)`.
    pub async fn dollar_unit(&mut self) -> Result<Option<TextUnit>> {
        let index = self.index();
        let location = match self.consume_char_if(|c| c == '$').await? {
            None => return Ok(None),
            Some(c) => c.location.clone(),
        };

        if let Some(result) = self.raw_param(index).await? {
            return Ok(Some(result));
        }

        if let Some(result) = self.braced_param(location).await? {
            return Ok(Some(TextUnit::BracedParam(result)));
        };

        if let Some(result) = self.arithmetic_expansion(index).await? {
            return Ok(Some(result));
        };

        if let Some(result) = self.command_substitution(index).await? {
            return Ok(Some(result));
        }

        // TODO maybe reject unrecognized dollar unit?
        self.rewind(index);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::lex::Lexer;
    use crate::parser::lex::WordContext;
    use crate::source::Source;
    use crate::syntax::Literal;
    use crate::syntax::Text;
    use assert_matches::assert_matches;
    use futures_executor::block_on;

    #[test]
    fn lexer_dollar_unit_no_dollar() {
        let mut lexer = Lexer::from_memory("foo", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.dollar_unit()).unwrap();
        assert_eq!(result, None);

        let mut lexer = Lexer::from_memory("()", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.dollar_unit()).unwrap();
        assert_eq!(result, None);
        assert_eq!(block_on(lexer.peek_char()), Ok(Some('(')));

        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.dollar_unit()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn lexer_dollar_unit_dollar_followed_by_non_special() {
        let mut lexer = Lexer::from_memory("$;", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.dollar_unit()).unwrap();
        assert_eq!(result, None);
        assert_eq!(block_on(lexer.peek_char()), Ok(Some('$')));

        let mut lexer = Lexer::from_memory("$&", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.dollar_unit()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn lexer_dollar_unit_raw_special_parameter() {
        let mut lexer = Lexer::from_memory("$0", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.dollar_unit()).unwrap().unwrap();
        assert_matches!(result, TextUnit::RawParam { name, span } => {
            assert_eq!(name, "0");
            assert_eq!(*span.code.value.borrow(), "$0");
            assert_eq!(span.code.start_line_number.get(), 1);
            assert_eq!(span.code.source, Source::Unknown);
            assert_eq!(span.range, 0..2);
        });
        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_dollar_unit_command_substitution() {
        let mut lexer = Lexer::from_memory("$()", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.dollar_unit()).unwrap().unwrap();
        assert_matches!(result, TextUnit::CommandSubst { content, span } => {
            assert_eq!(*span.code.value.borrow(), "$()");
            assert_eq!(span.code.start_line_number.get(), 1);
            assert_eq!(span.code.source, Source::Unknown);
            assert_eq!(span.range, 0..3);
            assert_eq!(content, "");
        });
        assert_eq!(block_on(lexer.peek_char()), Ok(None));

        let mut lexer = Lexer::from_memory("$( foo bar )", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.dollar_unit()).unwrap().unwrap();
        assert_matches!(result, TextUnit::CommandSubst { content, span } => {
            assert_eq!(*span.code.value.borrow(), "$( foo bar )");
            assert_eq!(span.code.start_line_number.get(), 1);
            assert_eq!(span.code.source, Source::Unknown);
            assert_eq!(span.range, 0..12);
            assert_eq!(content, " foo bar ");
        });
        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_dollar_unit_arithmetic_expansion() {
        let mut lexer = Lexer::from_memory("$((1))", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.dollar_unit()).unwrap().unwrap();
        assert_matches!(result, TextUnit::Arith { content, span } => {
            assert_eq!(content, Text(vec![Literal('1')]));
            assert_eq!(*span.code.value.borrow(), "$((1))");
            assert_eq!(span.code.start_line_number.get(), 1);
            assert_eq!(span.code.source, Source::Unknown);
            assert_eq!(span.range, 0..6);
        });
        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }

    #[test]
    fn lexer_dollar_unit_line_continuation() {
        let mut lexer = Lexer::from_memory("$\\\n\\\n0", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let result = block_on(lexer.dollar_unit()).unwrap().unwrap();
        assert_matches!(result, TextUnit::RawParam { name, .. } => {
            assert_eq!(name, "0");
        });
        assert_eq!(block_on(lexer.peek_char()), Ok(None));
    }
}
