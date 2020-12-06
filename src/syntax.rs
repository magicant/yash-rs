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

//! Shell command language syntax.
//!
//! TODO Elaborate

use itertools::Itertools;
use std::fmt;

// TODO Support full syntax
#[derive(Debug)]
pub struct Command {
    pub content: String,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.content)
    }
}

/// Token that may involve expansion.
///
/// It depends on context whether an empty word is valid or not. It is your responsibility to
/// ensure a word is non-empty in a context where it cannot.
#[derive(Debug)]
pub struct Word(pub String); // TODO Redefine as a vector of word elements.

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Command that involves assignments, redirections, and word expansions.
///
/// In the shell language syntax, a valid simple command must contain at least one of assignments,
/// redirections, and words. The parser must not produce a completely empty simple command.
#[derive(Debug)]
pub struct SimpleCommand {
    pub words: Vec<Word>,
    // TODO Assignments and redirections
}

impl fmt::Display for SimpleCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.words.is_empty() {
            return Ok(());
        }
        write!(f, "{}", self.words.iter().format(" "))
    }
}
