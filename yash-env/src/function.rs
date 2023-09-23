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

    /// Optional location where this function was made read-only.
    ///
    /// If this function is not read-only, `read_only_location` is `None`.
    /// Otherwise, `read_only_location` is the location of the simple command
    /// that executed the `readonly` built-in that made this function read-only.
    pub read_only_location: Option<Location>,
}

impl Function {
    /// Creates a new function.
    ///
    /// This is a convenience function for constructing a `Function` object.
    /// The `read_only_location` is set to `None`.
    #[inline]
    #[must_use]
    pub fn new<N: Into<String>, C: Into<Rc<FullCompoundCommand>>>(
        name: N,
        body: C,
        origin: Location,
    ) -> Self {
        Function {
            name: name.into(),
            body: body.into(),
            origin,
            read_only_location: None,
        }
    }

    /// Makes the function read-only.
    ///
    /// This is a convenience function for doing
    /// `self.read_only_location = Some(location)` in a method chain.
    #[inline]
    #[must_use]
    pub fn make_read_only(mut self, location: Location) -> Self {
        self.read_only_location = Some(location);
        self
    }

    /// Whether this function is read-only or not.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only_location.is_some()
    }
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
        read_only_location: Option<Location>,
    ) -> HashEntry {
        HashEntry(Rc::new(Function {
            name,
            body,
            origin,
            read_only_location,
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
