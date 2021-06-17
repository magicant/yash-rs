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

//! Type definitions for functions.
//!
//! This module provides data types for defining shell functions.

use std::borrow::Borrow;
use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::rc::Rc;
use yash_syntax::source::Location;
use yash_syntax::syntax::FullCompoundCommand;

/// Definition of a function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    /// String that identifies the function.
    pub name: String,

    /// Command that is executed when the function is called.
    ///
    /// This is wrapped in `Rc` so that a function can be defined and executed
    /// without cloning the entire compound command.
    pub body: Rc<FullCompoundCommand>,

    /// Location of the function definition command that defined this function.
    pub origin: Location,

    /// Whether this function is read-only or not.
    ///
    /// A read-only function cannot be re-defined or unset.
    pub is_read_only: bool,
}

/// Wrapper of [`Function`] for inserting into a hash set.
///
/// A `HashEntry` wraps a `Function` in `Rc` so that the function can be
/// referred to even after the function has been removed from the environment.
/// The `Hash` and `PartialEq` implementation for `HashEntry` only compares
/// names.
#[derive(Clone, Debug, Eq)]
pub struct HashEntry(pub Rc<Function>);

impl HashEntry {
    /// Convenience method for creating a new function as a `HashEntry`.
    pub fn new(
        name: String,
        body: Rc<FullCompoundCommand>,
        origin: Location,
        is_read_only: bool,
    ) -> HashEntry {
        HashEntry(Rc::new(Function {
            name,
            body,
            origin,
            is_read_only,
        }))
    }
}

impl PartialEq for HashEntry {
    /// Compares the names of two hash entries.
    ///
    /// Members of [`Function`] other than `name` are not considered in this
    /// function.
    fn eq(&self, other: &HashEntry) -> bool {
        self.0.name == other.0.name
    }
}

impl Hash for HashEntry {
    /// Hashes the name of the function.
    ///
    /// Members of [`Function`] other than `name` are not considered in this
    /// function.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.name.hash(state)
    }
}

impl Borrow<str> for HashEntry {
    fn borrow(&self) -> &str {
        &self.0.name
    }
}

/// Collection of functions.
pub type FunctionSet = HashSet<HashEntry>;
