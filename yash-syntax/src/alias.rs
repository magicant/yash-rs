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

//! Defining aliases.
//!
//! This module provides data structures for defining aliases in the shell
//! execution environment.

use crate::source::Span;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::rc::Rc;

/// Name-value pair that defines an alias.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Alias {
    /// Name of the alias that is matched against a command word by the syntax parser.
    pub name: String,
    /// String that substitutes part of the source code when it is found to match the alias name.
    pub replacement: String,
    /// Whether this alias is a global alias or not.
    pub global: bool,
    /// Position of the word in the simple command that invoked the alias built-in to define this
    /// alias.
    pub origin: Span,
}

/// Wrapper of [`Alias`] for inserting into a hash set.
///
/// A `HashEntry` wraps an `Alias` in `Rc` so that the alias definition can be referred to even
/// after the definition is removed. The `Hash` and `PartialEq` implementation for `HashEntry`
/// compares only names.
///
/// ```
/// let mut entries = std::collections::HashSet::new();
/// let name = "foo";
/// let origin = yash_syntax::source::Span::dummy("");
/// let old = yash_syntax::alias::HashEntry::new(
///     name.to_string(), "old".to_string(), false, origin.clone());
/// let new = yash_syntax::alias::HashEntry::new(
///     name.to_string(), "new".to_string(), false, origin);
/// entries.insert(old);
/// let old = entries.replace(new).unwrap();
/// assert_eq!(old.0.replacement, "old");
/// assert_eq!(entries.get(name).unwrap().0.replacement, "new");
/// ```
#[derive(Clone, Debug, Eq)]
pub struct HashEntry(pub Rc<Alias>);

impl HashEntry {
    /// Convenience method for creating a new alias definition as `HashEntry`
    pub fn new(name: String, replacement: String, global: bool, origin: Span) -> HashEntry {
        HashEntry(Rc::new(Alias {
            name,
            replacement,
            global,
            origin,
        }))
    }
}

impl PartialEq for HashEntry {
    fn eq(&self, other: &HashEntry) -> bool {
        self.0.name == other.0.name
    }
}

impl Hash for HashEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.name.hash(state)
    }
}

impl Borrow<str> for HashEntry {
    fn borrow(&self) -> &str {
        &self.0.name
    }
}

/// Collection of aliases.
pub type AliasSet = HashSet<HashEntry>;
