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

//! Defining aliases
//!
//! This module provides data structures for defining aliases in the shell
//! execution environment.

use crate::Env;
use crate::source::Location;
use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::hash::Hasher;
use std::rc::Rc;

/// Name-value pair that defines an alias
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Alias {
    /// Name of the alias that is matched against a command word by the syntax parser
    pub name: String,
    /// String that substitutes part of the source code when it is found to match the alias name
    pub replacement: String,
    /// Whether this alias is a global alias or not
    pub global: bool,
    /// Location of the word in the simple command that invoked the alias built-in to define this
    /// alias
    pub origin: Location,
}

/// Wrapper of [`Alias`] for inserting into a hash set
///
/// A `HashEntry` wraps an `Alias` in `Rc` so that the alias definition can be referred to even
/// after the definition is removed. The `Hash` and `PartialEq` implementation for `HashEntry`
/// compares only names.
///
/// ```
/// let mut entries = std::collections::HashSet::new();
/// let name = "foo";
/// let origin = yash_env::source::Location::dummy("");
/// let old = yash_env::alias::HashEntry::new(
///     name.to_string(), "old".to_string(), false, origin.clone());
/// let new = yash_env::alias::HashEntry::new(
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
    pub fn new(name: String, replacement: String, global: bool, origin: Location) -> HashEntry {
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

/// Collection of aliases
pub type AliasSet = HashSet<HashEntry>;

/// Interface used by the parser to look up aliases
///
/// This trait is an abstract interface that represents an immutable collection
/// of aliases. The parser uses this trait to look up aliases when it encounters
/// a command word in a simple command.
pub trait Glossary: Debug {
    /// Looks up an alias by name.
    ///
    /// If an alias with the given name is found, it is returned. Otherwise, the
    /// return value is `None`.
    #[must_use]
    // This method returns an `Rc<Alias>` rather than `&Alias` so that the
    // implementation for `RefCell` below can return a value after releasing
    // the borrow of the inner glossary.
    fn look_up(&self, name: &str) -> Option<Rc<Alias>>;

    /// Returns whether the glossary is empty.
    ///
    /// If the glossary is empty, the parser can skip alias expansion. This
    /// method is used as a hint to optimize alias expansion.
    ///
    /// If the glossary has any aliases, it must return `false`.
    ///
    /// The default implementation returns `false`.
    #[must_use]
    fn is_empty(&self) -> bool {
        false
    }
}

impl<T: Glossary> Glossary for &T {
    fn look_up(&self, name: &str) -> Option<Rc<Alias>> {
        (**self).look_up(name)
    }
    fn is_empty(&self) -> bool {
        (**self).is_empty()
    }
}

impl<T: Glossary> Glossary for &mut T {
    fn look_up(&self, name: &str) -> Option<Rc<Alias>> {
        (**self).look_up(name)
    }
    fn is_empty(&self) -> bool {
        (**self).is_empty()
    }
}

impl Glossary for AliasSet {
    fn look_up(&self, name: &str) -> Option<Rc<Alias>> {
        self.get(name).map(|entry| entry.0.clone())
    }
    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

/// Empty glossary that does not contain any aliases
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct EmptyGlossary;

impl Glossary for EmptyGlossary {
    #[inline(always)]
    fn look_up(&self, _name: &str) -> Option<Rc<Alias>> {
        None
    }
    #[inline(always)]
    fn is_empty(&self) -> bool {
        true
    }
}

/// Allows a glossary to be wrapped in a `RefCell`.
///
/// This implementation's methods immutably borrow the inner glossary.
/// If the inner glossary is mutably borrowed at the same time, it panics.
impl<T: Glossary> Glossary for RefCell<T> {
    fn look_up(&self, name: &str) -> Option<Rc<Alias>> {
        self.borrow().look_up(name)
    }
    fn is_empty(&self) -> bool {
        self.borrow().is_empty()
    }
}

/// Allows to look up aliases in the environment.
///
/// This implementation delegates to `self.aliases`.
impl Glossary for Env {
    #[inline(always)]
    fn look_up(&self, name: &str) -> Option<Rc<Alias>> {
        self.aliases.look_up(name)
    }
    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }
}
