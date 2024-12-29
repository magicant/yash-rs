// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Implementation of declaration utility glossary for the environment

use crate::Env;
use yash_syntax::decl_util::Glossary;

/// Determines whether a command name is a declaration utility.
///
/// This implementation looks up the command name in `self.builtins` and returns
/// the value of `is_declaration_utility` if the built-in is found. Otherwise,
/// the command is not a declaration utility.
impl Glossary for Env {
    fn is_declaration_utility(&self, name: &str) -> Option<bool> {
        match self.builtins.get(name) {
            Some(builtin) => builtin.is_declaration_utility,
            None => Some(false),
        }
    }
}
