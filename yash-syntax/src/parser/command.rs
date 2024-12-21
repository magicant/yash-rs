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

//! Syntax parser for command
//!
//! Note that the detail parser for each type of commands is in another
//! dedicated module.

use super::core::Parser;
use super::core::Rec;
use super::core::Result;
use crate::syntax::Command;

impl Parser<'_, '_> {
    /// Parses a command.
    ///
    /// If there is no valid command at the current position, this function
    /// returns `Ok(Rec::Parsed(None))`.
    pub async fn command(&mut self) -> Result<Rec<Option<Command>>> {
        match self.simple_command().await? {
            Rec::AliasSubstituted => Ok(Rec::AliasSubstituted),
            Rec::Parsed(None) => self
                .full_compound_command()
                .await
                .map(|c| Rec::Parsed(c.map(Command::Compound))),
            Rec::Parsed(Some(c)) => self
                .short_function_definition(c)
                .await
                .map(|c| Rec::Parsed(Some(c))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::lex::Lexer;
    use super::super::lex::TokenId::EndOfInput;
    use super::*;
    use crate::source::Source;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn parser_command_simple() {
        let mut lexer = Lexer::from_memory("foo < bar", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.command().now_or_never().unwrap();
        let command = result.unwrap().unwrap().unwrap();
        assert_matches!(command, Command::Simple(c) => {
            assert_eq!(c.to_string(), "foo <bar");
        });

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_command_compound() {
        let mut lexer = Lexer::from_memory("(foo) < bar", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.command().now_or_never().unwrap();
        let command = result.unwrap().unwrap().unwrap();
        assert_matches!(command, Command::Compound(c) => {
            assert_eq!(c.to_string(), "(foo) <bar");
        });

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_command_function() {
        let mut lexer = Lexer::from_memory("fun () ( echo )", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.command().now_or_never().unwrap();
        let command = result.unwrap().unwrap().unwrap();
        assert_matches!(command, Command::Function(f) => {
            assert_eq!(f.to_string(), "fun() (echo)");
        });

        let next = parser.peek_token().now_or_never().unwrap().unwrap();
        assert_eq!(next.id, EndOfInput);
    }

    #[test]
    fn parser_command_eof() {
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let mut parser = Parser::new(&mut lexer);

        let result = parser.command().now_or_never().unwrap().unwrap();
        assert_eq!(result, Rec::Parsed(None));
    }
}
