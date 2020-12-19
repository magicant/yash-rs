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
mod lex;

use super::syntax::*;

pub use self::core::AsyncFnMut;
pub use self::core::AsyncFnOnce;
pub use self::core::Error;
pub use self::core::ErrorCause;
pub use self::core::Parser;
pub use self::core::Result;
pub use self::fill::Fill;
pub use self::fill::MissingHereDoc;
pub use self::lex::Lexer;
pub use self::lex::Token;

impl Parser<'_> {
    /// Parses a simple command.
    pub async fn simple_command(&mut self) -> Result<SimpleCommand<MissingHereDoc>> {
        // TODO Support assignments and redirections. Stop on a delimiter token.
        let mut words = vec![];
        loop {
            let token = self.take_token().await;
            if let Err(Error {
                cause: ErrorCause::EndOfInput,
                ..
            }) = token
            {
                break;
            }
            words.push(token?.word);
        }
        Ok(SimpleCommand {
            words,
            redirs: vec![],
        })
    }
}
