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

use crate::Env;
use crate::source::Location;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::fmt::Debug;
use std::fmt::Display;
use std::hash::Hash;
use std::hash::Hasher;
use std::iter::FusedIterator;
use std::pin::Pin;
use std::rc::Rc;
use thiserror::Error;

/// Trait for the body of a [`Function`]
pub trait FunctionBody<S>: Debug + Display {
    /// Executes the function body in the given environment.
    ///
    /// The implementation of this method is expected to update
    /// `env.exit_status` reflecting the result of the function execution.
    #[allow(async_fn_in_trait)] // We don't support Send
    async fn execute(&self, env: &mut Env<S>) -> crate::semantics::Result;
}

/// Dyn-compatible adapter for the [`FunctionBody`] trait
///
/// This is a dyn-compatible version of the [`FunctionBody`] trait.
///
/// This trait is automatically implemented for all types that implement
/// [`FunctionBody`].
pub trait FunctionBodyObject<S>: Debug + Display {
    /// Executes the function body in the given environment.
    ///
    /// The implementation of this method is expected to update
    /// `env.exit_status` reflecting the result of the function execution.
    fn execute<'a>(
        &'a self,
        env: &'a mut Env<S>,
    ) -> Pin<Box<dyn Future<Output = crate::semantics::Result> + 'a>>;
}

impl<S, T: FunctionBody<S> + ?Sized> FunctionBodyObject<S> for T {
    fn execute<'a>(
        &'a self,
        env: &'a mut Env<S>,
    ) -> Pin<Box<dyn Future<Output = crate::semantics::Result> + 'a>> {
        Box::pin(self.execute(env))
    }
}

/// Definition of a function.
pub struct Function<S> {
    /// String that identifies the function.
    pub name: String,

    /// Command that is executed when the function is called.
    ///
    /// This is wrapped in `Rc` so that we don't have to clone the entire
    /// command when we define a function. The function definition command only
    /// clones the `Rc` object from the abstract syntax tree to create a
    /// `Function` object.
    pub body: Rc<dyn FunctionBodyObject<S>>,

    /// Location of the function definition command that defined this function.
    pub origin: Location,

    /// Optional location where this function was made read-only.
    ///
    /// If this function is not read-only, `read_only_location` is `None`.
    /// Otherwise, `read_only_location` is the location of the simple command
    /// that executed the `readonly` built-in that made this function read-only.
    pub read_only_location: Option<Location>,
}

impl<S> Function<S> {
    /// Creates a new function.
    ///
    /// This is a convenience function for constructing a `Function` object.
    /// The `read_only_location` is set to `None`.
    #[inline]
    #[must_use]
    pub fn new<N: Into<String>, B: Into<Rc<dyn FunctionBodyObject<S>>>>(
        name: N,
        body: B,
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

// Not derived automatically because S may not implement Clone
impl<S> Clone for Function<S> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            body: self.body.clone(),
            origin: self.origin.clone(),
            read_only_location: self.read_only_location.clone(),
        }
    }
}

/// Compares two functions for equality.
///
/// Two functions are considered equal if all their members are equal.
/// This includes comparing the `body` members by pointer equality.
impl<S> PartialEq for Function<S> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && Rc::ptr_eq(&self.body, &other.body)
            && self.origin == other.origin
            && self.read_only_location == other.read_only_location
    }
}

impl<S> Eq for Function<S> {}

// Not derived automatically because S may not implement Debug
impl<S> std::fmt::Debug for Function<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Function")
            .field("name", &self.name)
            .field("body", &self.body)
            .field("origin", &self.origin)
            .field("read_only_location", &self.read_only_location)
            .finish()
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
#[derive(Debug)]
struct HashEntry<S>(Rc<Function<S>>);

// Not derived automatically because S may not implement Clone
impl<S> Clone for HashEntry<S> {
    fn clone(&self) -> Self {
        HashEntry(Rc::clone(&self.0))
    }
}

impl<S> PartialEq for HashEntry<S> {
    /// Compares the names of two hash entries.
    ///
    /// Members of [`Function`] other than `name` are not considered in this
    /// function.
    fn eq(&self, other: &HashEntry<S>) -> bool {
        self.0.name == other.0.name
    }
}

// Not derived automatically because S may not implement Eq
impl<S> Eq for HashEntry<S> {}

impl<S> Hash for HashEntry<S> {
    /// Hashes the name of the function.
    ///
    /// Members of [`Function`] other than `name` are not considered in this
    /// function.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.name.hash(state)
    }
}

impl<S> Borrow<str> for HashEntry<S> {
    fn borrow(&self) -> &str {
        &self.0.name
    }
}

/// Collection of functions.
#[derive(Debug)]
pub struct FunctionSet<S> {
    entries: HashSet<HashEntry<S>>,
}

// Not derived automatically because S may not implement Clone
impl<S> Clone for FunctionSet<S> {
    fn clone(&self) -> Self {
        let entries = self.entries.clone();
        Self { entries }
    }
}

// Not derived automatically because S may not implement Default
impl<S> Default for FunctionSet<S> {
    fn default() -> Self {
        let entries = HashSet::default();
        Self { entries }
    }
}

/// Error redefining a read-only function.
#[derive(Error)]
#[error("cannot redefine read-only function `{}`", .existing.name)]
#[non_exhaustive]
pub struct DefineError<S> {
    /// Existing read-only function
    pub existing: Rc<Function<S>>,
    /// New function that tried to redefine the existing function
    pub new: Rc<Function<S>>,
}

// Not derived automatically because S may not implement Clone, Debug, or PartialEq
impl<S> Clone for DefineError<S> {
    fn clone(&self) -> Self {
        Self {
            existing: self.existing.clone(),
            new: self.new.clone(),
        }
    }
}

impl<S> Debug for DefineError<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefineError")
            .field("existing", &self.existing)
            .field("new", &self.new)
            .finish()
    }
}

impl<S> PartialEq for DefineError<S> {
    fn eq(&self, other: &Self) -> bool {
        self.existing == other.existing && self.new == other.new
    }
}

impl<S> Eq for DefineError<S> {}

/// Error unsetting a read-only function.
#[derive(Error)]
#[error("cannot unset read-only function `{}`", .existing.name)]
#[non_exhaustive]
pub struct UnsetError<S> {
    /// Existing read-only function
    pub existing: Rc<Function<S>>,
}

// Not derived automatically because S may not implement Clone, Debug, or PartialEq
impl<S> Clone for UnsetError<S> {
    fn clone(&self) -> Self {
        Self {
            existing: self.existing.clone(),
        }
    }
}

impl<S> Debug for UnsetError<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnsetError")
            .field("existing", &self.existing)
            .finish()
    }
}

impl<S> PartialEq for UnsetError<S> {
    fn eq(&self, other: &Self) -> bool {
        self.existing == other.existing
    }
}

impl<S> Eq for UnsetError<S> {}

/// Unordered iterator over functions in a function set.
///
/// This iterator is created by [`FunctionSet::iter`].
#[derive(Debug)]
pub struct Iter<'a, S> {
    inner: std::collections::hash_set::Iter<'a, HashEntry<S>>,
}

// Not derived automatically because S may not implement Clone
impl<S> Clone for Iter<'_, S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S> FunctionSet<S> {
    /// Creates a new empty function set.
    #[must_use]
    pub fn new() -> Self {
        FunctionSet::default()
    }

    /// Returns the function with the given name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Rc<Function<S>>> {
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
    pub fn define<F: Into<Rc<Function<S>>>>(
        &mut self,
        function: F,
    ) -> Result<Option<Rc<Function<S>>>, DefineError<S>> {
        #[allow(clippy::mutable_key_type)]
        fn inner<S>(
            entries: &mut HashSet<HashEntry<S>>,
            new: Rc<Function<S>>,
        ) -> Result<Option<Rc<Function<S>>>, DefineError<S>> {
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
    pub fn unset(&mut self, name: &str) -> Result<Option<Rc<Function<S>>>, UnsetError<S>> {
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
    pub fn iter(&self) -> Iter<'_, S> {
        let inner = self.entries.iter();
        Iter { inner }
    }
}

impl<'a, S> Iterator for Iter<'a, S> {
    type Item = &'a Rc<Function<S>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|entry| &entry.0)
    }
}

impl<S> ExactSizeIterator for Iter<'_, S> {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<S> FusedIterator for Iter<'_, S> {}

impl<'a, S> IntoIterator for &'a FunctionSet<S> {
    type Item = &'a Rc<Function<S>>;
    type IntoIter = Iter<'a, S>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct FunctionBodyStub;

    impl std::fmt::Display for FunctionBodyStub {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            unreachable!()
        }
    }
    impl<S> FunctionBody<S> for FunctionBodyStub {
        async fn execute(&self, _: &mut Env<S>) -> crate::semantics::Result {
            unreachable!()
        }
    }

    fn function_body_stub<S>() -> Rc<dyn FunctionBodyObject<S>> {
        Rc::new(FunctionBodyStub)
    }

    #[test]
    fn defining_new_function() {
        let mut set = FunctionSet::<()>::new();
        let function = Rc::new(Function::new(
            "foo",
            function_body_stub(),
            Location::dummy("foo"),
        ));

        let result = set.define(function.clone());
        assert_eq!(result, Ok(None));
        assert_eq!(set.get("foo"), Some(&function));
    }

    #[test]
    fn redefining_existing_function() {
        let mut set = FunctionSet::<()>::new();
        let function1 = Rc::new(Function::new(
            "foo",
            function_body_stub(),
            Location::dummy("foo 1"),
        ));
        let function2 = Rc::new(Function::new(
            "foo",
            function_body_stub(),
            Location::dummy("foo 2"),
        ));
        set.define(function1.clone()).unwrap();

        let result = set.define(function2.clone());
        assert_eq!(result, Ok(Some(function1)));
        assert_eq!(set.get("foo"), Some(&function2));
    }

    #[test]
    fn redefining_readonly_function() {
        let mut set = FunctionSet::<()>::new();
        let function1 = Rc::new(
            Function::new("foo", function_body_stub(), Location::dummy("foo 1"))
                .make_read_only(Location::dummy("readonly")),
        );
        let function2 = Rc::new(Function::new(
            "foo",
            function_body_stub(),
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
        let mut set = FunctionSet::<()>::new();
        let function = Rc::new(Function::new(
            "foo",
            function_body_stub(),
            Location::dummy("foo"),
        ));
        set.define(function.clone()).unwrap();

        let result = set.unset("foo").unwrap();
        assert_eq!(result, Some(function));
        assert_eq!(set.get("foo"), None);
    }

    #[test]
    fn unsetting_nonexisting_function() {
        let mut set = FunctionSet::<()>::new();

        let result = set.unset("foo").unwrap();
        assert_eq!(result, None);
        assert_eq!(set.get("foo"), None);
    }

    #[test]
    fn unsetting_readonly_function() {
        let mut set = FunctionSet::<()>::new();
        let function = Rc::new(
            Function::new("foo", function_body_stub(), Location::dummy("foo"))
                .make_read_only(Location::dummy("readonly")),
        );
        set.define(function.clone()).unwrap();

        let error = set.unset("foo").unwrap_err();
        assert_eq!(error.existing, function);
    }

    #[test]
    fn iteration() {
        let mut set = FunctionSet::<()>::new();
        let function1 = Rc::new(Function::new(
            "foo",
            function_body_stub(),
            Location::dummy("foo"),
        ));
        let function2 = Rc::new(Function::new(
            "bar",
            function_body_stub(),
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
