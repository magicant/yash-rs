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
use std::iter::FusedIterator;
use std::rc::Rc;
use thiserror::Error;
use yash_syntax::source::Location;
use yash_syntax::syntax::FullCompoundCommand;

/// Definition of a function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    /// String that identifies the function.
    pub name: String,

    /// Command that is executed when the function is called.
    ///
    /// This is wrapped in `Rc` so that we don't have to clone the entire
    /// compound command when we define a function. The function definition
    /// command only clones the `Rc` object from the abstract syntax tree.
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
/// A `HashEntry` wraps a `Function` in `Rc` so that the `Function` object can
/// outlive the execution of the function which may redefine or unset the
/// function itself. A simple command that executes the function clones the
/// `Rc` object from the function set and retains it until the command
/// terminates.
///
/// The `Hash` and `PartialEq` implementation for `HashEntry` only compares
/// the names of the functions.
#[derive(Clone, Debug, Eq)]
struct HashEntry(Rc<Function>);

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
#[derive(Clone, Debug, Default)]
pub struct FunctionSet {
    entries: HashSet<HashEntry>,
}

/// Error redefining a read-only function.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cannot redefine read-only function `{}`", .existing.name)]
#[non_exhaustive]
pub struct DefineError {
    /// Existing read-only function
    pub existing: Rc<Function>,
    /// New function that tried to redefine the existing function
    pub new: Rc<Function>,
}

/// Error unsetting a read-only function.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cannot unset read-only function `{}`", .existing.name)]
#[non_exhaustive]
pub struct UnsetError {
    /// Existing read-only function
    pub existing: Rc<Function>,
}

/// Unordered iterator over functions in a function set.
///
/// This iterator is created by [`FunctionSet::iter`].
#[derive(Clone, Debug)]
pub struct Iter<'a> {
    inner: std::collections::hash_set::Iter<'a, HashEntry>,
}

impl FunctionSet {
    /// Creates a new empty function set.
    #[must_use]
    pub fn new() -> Self {
        FunctionSet::default()
    }

    /// Returns the function with the given name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Rc<Function>> {
        self.entries.get(name).map(|entry| &entry.0)
    }

    /// Returns the number of functions in the set.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the set contains no functions.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Inserts a function into the set.
    ///
    /// If a function with the same name already exists, it is replaced and
    /// returned unless it is read-only, in which case `DefineError` is
    /// returned.
    pub fn define<F: Into<Rc<Function>>>(
        &mut self,
        function: F,
    ) -> Result<Option<Rc<Function>>, DefineError> {
        fn inner(
            entries: &mut HashSet<HashEntry>,
            new: Rc<Function>,
        ) -> Result<Option<Rc<Function>>, DefineError> {
            // TODO Use Option::is_some_and
            match entries.get(new.name.as_str()) {
                Some(existing) if existing.0.is_read_only() => Err(DefineError {
                    existing: Rc::clone(&existing.0),
                    new,
                }),

                _ => Ok(entries.replace(HashEntry(new)).map(|entry| entry.0)),
            }
        }
        inner(&mut self.entries, function.into())
    }

    /// Removes a function from the set.
    ///
    /// This function returns the previously defined function if it exists.
    /// However, if the function is read-only, `UnsetError` is returned.
    pub fn unset(&mut self, name: &str) -> Result<Option<Rc<Function>>, UnsetError> {
        // TODO Use Option::is_some_and
        match self.entries.get(name) {
            Some(entry) if entry.0.is_read_only() => Err(UnsetError {
                existing: Rc::clone(&entry.0),
            }),

            _ => Ok(self.entries.take(name).map(|entry| entry.0)),
        }
    }

    /// Returns an iterator over functions in the set.
    ///
    /// The order of iteration is not specified.
    pub fn iter(&self) -> Iter {
        let inner = self.entries.iter();
        Iter { inner }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Rc<Function>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|entry| &entry.0)
    }
}

impl ExactSizeIterator for Iter<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl FusedIterator for Iter<'_> {}

impl<'a> IntoIterator for &'a FunctionSet {
    type Item = &'a Rc<Function>;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defining_new_function() {
        let mut set = FunctionSet::new();
        let function = Rc::new(Function::new(
            "foo",
            "{ :; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("foo"),
        ));

        let result = set.define(function.clone());
        assert_eq!(result, Ok(None));
        assert_eq!(set.get("foo"), Some(&function));
    }

    #[test]
    fn redefining_existing_function() {
        let mut set = FunctionSet::new();
        let function1 = Rc::new(Function::new(
            "foo",
            "{ echo 1; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("foo 1"),
        ));
        let function2 = Rc::new(Function::new(
            "foo",
            "{ echo 2; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("foo 2"),
        ));
        set.define(function1.clone()).unwrap();

        let result = set.define(function2.clone());
        assert_eq!(result, Ok(Some(function1)));
        assert_eq!(set.get("foo"), Some(&function2));
    }

    #[test]
    fn redefining_readonly_function() {
        let mut set = FunctionSet::new();
        let function1 = Rc::new(
            Function::new(
                "foo",
                "{ echo 1; }".parse::<FullCompoundCommand>().unwrap(),
                Location::dummy("foo 1"),
            )
            .make_read_only(Location::dummy("readonly")),
        );
        let function2 = Rc::new(Function::new(
            "foo",
            "{ echo 2; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("foo 2"),
        ));
        set.define(function1.clone()).unwrap();

        let error = set.define(function2.clone()).unwrap_err();
        assert_eq!(error.existing, function1);
        assert_eq!(error.new, function2);
        assert_eq!(set.get("foo"), Some(&function1));
    }

    #[test]
    fn unsetting_existing_function() {
        let mut set = FunctionSet::new();
        let function = Rc::new(Function::new(
            "foo",
            "{ :; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("foo"),
        ));
        set.define(function.clone()).unwrap();

        let result = set.unset("foo").unwrap();
        assert_eq!(result, Some(function));
        assert_eq!(set.get("foo"), None);
    }

    #[test]
    fn unsetting_nonexisting_function() {
        let mut set = FunctionSet::new();

        let result = set.unset("foo").unwrap();
        assert_eq!(result, None);
        assert_eq!(set.get("foo"), None);
    }

    #[test]
    fn unsetting_readonly_function() {
        let mut set = FunctionSet::new();
        let function = Rc::new(
            Function::new(
                "foo",
                "{ :; }".parse::<FullCompoundCommand>().unwrap(),
                Location::dummy("foo"),
            )
            .make_read_only(Location::dummy("readonly")),
        );
        set.define(function.clone()).unwrap();

        let error = set.unset("foo").unwrap_err();
        assert_eq!(error.existing, function);
    }

    #[test]
    fn iteration() {
        let mut set = FunctionSet::new();
        let function1 = Rc::new(Function::new(
            "foo",
            "{ echo 1; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("foo"),
        ));
        let function2 = Rc::new(Function::new(
            "bar",
            "{ echo 2; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("bar"),
        ));
        set.define(function1.clone()).unwrap();
        set.define(function2.clone()).unwrap();

        let functions = set.iter().collect::<Vec<_>>();
        assert!(
            functions[..] == [&function1, &function2] || functions[..] == [&function2, &function1],
            "{functions:?}"
        );
    }
}
