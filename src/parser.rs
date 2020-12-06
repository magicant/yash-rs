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

use super::syntax::*;

/// Set of intermediate data used in parsing.
pub struct Parser {
    input: String,
}

impl Parser {
    /// Creates a new parser.
    pub fn new(input: String) -> Parser {
        Parser { input }
    }
    pub fn parse_command(&mut self) -> Command {
        Command {
            content: std::mem::replace(&mut self.input, String::new()),
        }
    }
}
