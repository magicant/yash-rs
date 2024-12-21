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

//! Defining declaration utilities
//!
//! TODO Elaborate on this module

use std::cell::RefCell;
use std::fmt::Debug;

/// Interface used by the parser to tell if a command name is a declaration utility
///
/// TODO Elaborate on this trait
pub trait Glossary: Debug {
    /// Returns whether the given command name is a declaration utility.
    ///
    /// TODO Elaborate on this method
    fn is_declaration_utility(&self, name: &str) -> Option<bool>;
}

/// Empty glossary that does not recognize any command name as a declaration utility
///
/// When this glossary is used, the parser recognizes no command name as a
/// declaration utility. Note that this does not conform to POSIX.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct EmptyGlossary;

impl Glossary for EmptyGlossary {
    #[inline(always)]
    fn is_declaration_utility(&self, _name: &str) -> Option<bool> {
        Some(false)
    }
}

/// Glossary that recognizes declaration utilities defined by POSIX
///
/// TODO Elaborate on this glossary
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct PosixGlossary;

impl Glossary for PosixGlossary {
    fn is_declaration_utility(&self, name: &str) -> Option<bool> {
        match name {
            "export" | "readonly" => Some(true),
            "command" => None,
            _ => Some(false),
        }
    }
}

impl<T: Glossary> Glossary for &T {
    fn is_declaration_utility(&self, name: &str) -> Option<bool> {
        (**self).is_declaration_utility(name)
    }
}

impl<T: Glossary> Glossary for &mut T {
    fn is_declaration_utility(&self, name: &str) -> Option<bool> {
        (**self).is_declaration_utility(name)
    }
}

/// Allows a glossary to be wrapped in a `RefCell`.
///
/// This implementation's methods immutably borrow the inner glossary.
/// If the inner glossary is mutably borrowed at the same time, it panics.
impl<T: Glossary> Glossary for RefCell<T> {
    fn is_declaration_utility(&self, name: &str) -> Option<bool> {
        self.borrow().is_declaration_utility(name)
    }
}
