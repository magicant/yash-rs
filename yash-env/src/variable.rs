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

//! Items for shell variables
//!
//! A [`Variable`] is a named parameter that can be assigned and exported. It is
//! defined in a context of a variable set. A [`VariableSet`] is a stack of
//! contexts that can be pushed and popped. Each context has a map of
//! name-variable pairs that effectively manages the variables.
//!
//! # Variable sets and contexts
//!
//! The variable set is a component of the shell environment ([`Env`]). It
//! contains a non-empty stack of contexts. The first context in the stack is
//! called the _base context_, and it is always present. Other contexts can be
//! pushed and popped on a last-in-first-out basis.
//!
//! Each context is a map of name-variable pairs. Variables in a context hide
//! those with the same name in lower contexts. You cannot access such hidden
//! variables until the hiding variables are removed or the context containing
//! them is popped.
//!
//! There are two types of [`Context`]s: regular and volatile. A regular context
//! is the default context type and may have positional parameters. A volatile
//! context is used for holding temporary variables when executing a built-in or
//! function. The context types and [`Scope`] affect the behavior of variable
//! assignment. The base context is always a regular context.
//!
//! Note that the notion of name-variable pairs is directly implemented in the
//! [`VariableSet`] struct, and is not visible in the [`Context`] enum.
//!
//! ## Context guards
//!
//! This module provides guards to ensure contexts are pushed and popped
//! correctly. The push function returns a guard that will pop the context when
//! dropped. Implementing `Deref` and `DerefMut`, the guard allows access to the
//! borrowed variable set or environment. To push a new context and acquire a
//! guard, use [`VariableSet::push_context`] or [`Env::push_context`].
//!
//! # Variables
//!
//! An instance of [`Variable`] represents the value and attributes of a shell
//! variable. Although all the fields of a variable are public, you cannot
//! obtain a mutable reference to a variable from a variable set directly. You
//! need to use [`VariableRefMut`] to modify a variable.
//!
//! ## Variable names and initial values
//!
//! This module defines constants for the names and initial values of some
//! variables. The constants are used in the shell initialization process to
//! create and assign the variables. The documentation for each name constant
//! describes the variable's purpose and initial value.
//!
//! # Examples
//!
//! ```
//! use yash_env::variable::{Context, Scope, VariableSet};
//! let mut set = VariableSet::new();
//!
//! // Define a variable in the base context
//! let mut var = set.get_or_new("foo", Scope::Global);
//! var.assign("hello", None).unwrap();
//!
//! // Push a new context
//! let mut guard = set.push_context(Context::default());
//!
//! // The variable is still visible
//! assert_eq!(guard.get("foo").unwrap().value, Some("hello".into()));
//!
//! // Defining a new variable in the new context hides the previous variable
//! let mut var = guard.get_or_new("foo", Scope::Local);
//! var.assign("world", None).unwrap();
//!
//! // The new variable is visible
//! assert_eq!(guard.get("foo").unwrap().value, Some("world".into()));
//!
//! // Pop the context
//! drop(guard);
//!
//! // The previous variable is visible again
//! assert_eq!(set.get("foo").unwrap().value, Some("hello".into()));
//! ```

#[cfg(doc)]
use crate::Env;
use crate::semantics::Field;
use itertools::Itertools;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::ffi::CString;
use std::fmt::Write;
use std::hash::Hash;
use std::iter::FusedIterator;
use thiserror::Error;
use yash_syntax::source::Location;

mod value;

pub use self::value::QuotedValue;
pub use self::value::Value::{self, Array, Scalar};

mod quirk;

pub use self::quirk::Expansion;
pub use self::quirk::Quirk;

mod main;

pub use self::main::AssignError;
pub use self::main::Variable;
pub use self::main::VariableRefMut;

mod constants;

// Export variable name and initial value constants
pub use self::constants::*;

#[derive(Clone, Debug, Eq, PartialEq)]
struct VariableInContext {
    variable: Variable,
    context_index: usize,
}

/// Positional parameters
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PositionalParams {
    /// Values of positional parameters
    pub values: Vec<String>,
    /// Location of the last modification of positional parameters
    pub last_modified_location: Option<Location>,
}

impl PositionalParams {
    /// Creates a `PositionalParams` instance from fields.
    ///
    /// The given iterator should be a non-empty sequence of fields. The first
    /// field is the name of the command whose origin is used as the
    /// `last_modified_location`. The rest of the fields are the values of
    /// positional parameters.
    pub fn from_fields<I>(fields: I) -> Self
    where
        I: IntoIterator<Item = Field>,
    {
        let mut fields = fields.into_iter();
        let last_modified_location = fields.next().map(|field| field.origin);
        let values = fields.map(|field| field.value).collect();
        Self {
            values,
            last_modified_location,
        }
    }
}

/// Variable context
///
/// This enum defines the type of a context. The context type affects the
/// behavior of variable [assignment](VariableRefMut::assign). A regular context
/// is the default context type and may have positional parameters. A volatile
/// context is used for holding temporary variables when executing a built-in or
/// function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Context {
    /// Context for normal assignments.
    ///
    /// The base context is a regular context. Every function invocation also
    /// creates a regular context for local assignments and positional
    /// parameters.
    Regular { positional_params: PositionalParams },

    /// Context for temporary assignments.
    ///
    /// A volatile context is used for holding temporary variables when
    /// executing a built-in or function.
    Volatile,
}

impl Default for Context {
    fn default() -> Self {
        Context::Regular {
            positional_params: Default::default(),
        }
    }
}

/// Collection of variables.
///
/// See the [module documentation](self) for details.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariableSet {
    /// Hash map containing all variables.
    ///
    /// The value of a hash map entry is a stack of variables defined in
    /// contexts, sorted in the ascending order of the context index.
    ///
    /// Having the variables of all the contexts in this single hash map makes
    /// the variable search faster than having a separate hash map for each
    /// context.
    all_variables: HashMap<String, Vec<VariableInContext>>,

    /// Stack of contexts.
    ///
    /// The stack can never be empty since the base context is always the first
    /// item.
    contexts: Vec<Context>,
}

impl Default for VariableSet {
    fn default() -> Self {
        VariableSet {
            all_variables: Default::default(),
            contexts: vec![Context::default()],
        }
    }
}

/// Choice of a context in which a variable is assigned or searched for.
///
/// For the meaning of the variants of this enum, see the docs for the functions
/// that use it: [`VariableRefMut::assign`] and [`VariableSet::iter`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    Global,
    Local,
    Volatile,
}

/// Error that occurs when unsetting a read-only variable
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cannot unset read-only variable `{name}`")]
pub struct UnsetError<'a> {
    /// Variable name.
    pub name: &'a str,
    /// Location where the existing variable was made read-only.
    pub read_only_location: &'a Location,
}

/// Iterator of variables
///
/// [`VariableSet::iter`] returns this iterator.
#[derive(Clone, Debug)]
pub struct Iter<'a> {
    inner: std::collections::hash_map::Iter<'a, String, Vec<VariableInContext>>,
    min_context_index: usize,
}

impl VariableSet {
    /// Creates an empty variable set.
    #[must_use]
    pub fn new() -> VariableSet {
        Default::default()
    }

    /// Gets a reference to the variable with the specified name.
    ///
    /// This method searches for a variable of the specified name and returns a
    /// reference to it if found. If variables with the same name are defined in
    /// multiple contexts, the one in the topmost context is considered
    /// _visible_ and returned. To limit the search to the local context, use
    /// [`get_scoped`](Self::get_scoped).
    ///
    /// You cannot retrieve positional parameters using this function.
    /// See [`positional_params`](Self::positional_params).
    #[must_use]
    pub fn get<N>(&self, name: &N) -> Option<&Variable>
    where
        String: Borrow<N>,
        N: Hash + Eq + ?Sized,
    {
        Some(&self.all_variables.get(name)?.last()?.variable)
    }

    /// Computes the index of the topmost regular context.
    fn index_of_topmost_regular_context(contexts: &[Context]) -> usize {
        contexts
            .iter()
            .rposition(|context| matches!(context, Context::Regular { .. }))
            .expect("base context has gone")
    }

    /// Computes the index of the context that matches the specified scope.
    fn index_of_context(scope: Scope, contexts: &[Context]) -> usize {
        match scope {
            Scope::Global => 0,
            Scope::Local => Self::index_of_topmost_regular_context(contexts),
            Scope::Volatile => Self::index_of_topmost_regular_context(contexts) + 1,
        }
    }

    /// Returns a reference to the variable with the specified name.
    ///
    /// This method searches for a variable of the specified name and returns a
    /// reference to it if found. The `scope` parameter determines the context
    /// the variable is searched for:
    ///
    /// - If the scope is `Global`, the variable is searched for in all contexts
    ///   from the topmost to the base context.
    /// - If the scope is `Local`, the variable is searched for from the topmost
    ///   to the topmost regular context.
    /// - If the scope is `Volatile`, the variable is searched for in volatile
    ///   contexts above the topmost regular context.
    ///
    /// `get_scoped` with `Scope::Global` is equivalent to [`get`](Self::get).
    ///
    /// You cannot retrieve positional parameters using this function.
    /// See [`positional_params`](Self::positional_params).
    #[must_use]
    pub fn get_scoped<N>(&self, name: &N, scope: Scope) -> Option<&Variable>
    where
        String: Borrow<N>,
        N: Hash + Eq + ?Sized,
    {
        let index = Self::index_of_context(scope, &self.contexts);
        self.all_variables
            .get(name)?
            .last()
            .filter(|vic| vic.context_index >= index)
            .map(|vic| &vic.variable)
    }

    /// Gets a mutable reference to the variable with the specified name.
    ///
    /// You use this method to create or modify a variable.
    /// This method searches for a variable of the specified name, and returns
    /// a mutable reference to it if found. Otherwise, this method creates a new
    /// variable and returns a mutable reference to it. The `scope` parameter
    /// determines the context the variable is searched for or created in:
    ///
    /// - If the scope is `Global`, an existing variable is searched for like
    ///   [`get`](Self::get). If a variable is found in a [regular] context, the
    ///   variable is returned. If there is no variable, a new defaulted
    ///   variable is created in the base context and returned.
    ///   - If a variable is in a [volatile] context, this method removes the
    ///     variable from the volatile context and continues searching for a
    ///     variable in a lower context. If a variable is found in a regular
    ///     context, it is replaced with the variable removed from the volatile
    ///     context. Otherwise, the removed variable is moved to the base
    ///     context. In either case, the moved variable is returned.
    /// - If the scope is `Local`, the behavior is the same as `Global` except
    ///   that any contexts below the topmost [regular] context are ignored.
    ///   If a variable is found in the topmost regular context, the variable is
    ///   returned. If there is no variable, a new defaulted variable is created
    ///   in the topmost regular context and returned.
    ///   - If a variable is in a [volatile] context above the topmost regular
    ///     context, the variable is moved to the topmost regular context,
    ///     overwriting the existing variable if any. The moved variable is
    ///     returned.
    /// - If the scope is `Volatile`, this method requires the topmost context
    ///   to be [volatile]. Otherwise, this method will **panic!** If the
    ///   topmost context is volatile, an existing variable is searched for like
    ///   [`get`](Self::get). If a variable is found in the topmost context, the
    ///   variable is returned. If a variable is found in a lower context, the
    ///   variable is cloned to the topmost context and returned. If there is
    ///   no variable, a new defaulted variable is created in the topmost
    ///   context and returned.
    ///
    /// You cannot modify positional parameters using this method.
    /// See [`positional_params_mut`](Self::positional_params_mut).
    ///
    /// This method does not apply the [`AllExport`](crate::option::AllExport)
    /// option.  You need to [export](VariableRefMut::export) the variable
    /// yourself, or use [`Env::get_or_create_variable`] to get the option
    /// applied automatically.
    ///
    /// [regular]: Context::Regular
    /// [volatile]: Context::Volatile
    #[inline]
    pub fn get_or_new<S: Into<String>>(&mut self, name: S, scope: Scope) -> VariableRefMut {
        self.get_or_new_impl(name.into(), scope)
    }

    fn get_or_new_impl(&mut self, name: String, scope: Scope) -> VariableRefMut {
        let stack = match self.all_variables.entry(name) {
            Vacant(vacant) => vacant.insert(Vec::new()),
            Occupied(occupied) => occupied.into_mut(),
        };
        let context_index = match scope {
            Scope::Global => 0,
            Scope::Local => Self::index_of_topmost_regular_context(&self.contexts),
            Scope::Volatile => self.contexts.len() - 1,
        };

        match scope {
            Scope::Global | Scope::Local => 'branch: {
                let mut removed_volatile_variable = None;

                // Search the stack for a variable to return, and add one if not found.
                // If a variable is in a volatile context, temporarily move it to
                // removed_volatile_variable and put it in the target context before returning it.
                while let Some(var) = stack.last_mut() {
                    if var.context_index < context_index {
                        break;
                    }
                    match self.contexts[var.context_index] {
                        Context::Regular { .. } => {
                            if let Some(removed_volatile_variable) = removed_volatile_variable {
                                var.variable = removed_volatile_variable;
                            }
                            break 'branch;
                        }
                        Context::Volatile => {
                            removed_volatile_variable.get_or_insert(stack.pop().unwrap().variable);
                        }
                    }
                }

                stack.push(VariableInContext {
                    variable: removed_volatile_variable.unwrap_or_default(),
                    context_index,
                });
            }

            Scope::Volatile => {
                assert_eq!(
                    self.contexts[context_index],
                    Context::Volatile,
                    "no volatile context to store the variable",
                );
                if let Some(var) = stack.last() {
                    if var.context_index != context_index {
                        stack.push(VariableInContext {
                            variable: var.variable.clone(),
                            context_index,
                        });
                    }
                } else {
                    stack.push(VariableInContext {
                        variable: Variable::default(),
                        context_index,
                    });
                }
            }
        }

        VariableRefMut::from(&mut stack.last_mut().unwrap().variable)
    }

    /// Panics if the set contains any variable with an invalid context index.
    #[cfg(test)]
    fn assert_normalized(&self) {
        for context in self.all_variables.values() {
            for vars in context.windows(2) {
                assert!(
                    vars[0].context_index < vars[1].context_index,
                    "invalid context index: {vars:?}",
                );
            }
            if let Some(last) = context.last() {
                assert!(
                    last.context_index < self.contexts.len(),
                    "invalid context index: {last:?}",
                );
            }
        }
    }

    /// Gets the value of the specified scalar variable.
    ///
    /// This is a convenience function that retrieves the value of the specified
    /// scalar variable. If the variable is unset or an array, this method
    /// returns `None`.
    ///
    /// Note that this function does not apply any [`Quirk`] the variable may
    /// have. Use [`Variable::expand`] to apply quirks.
    #[must_use]
    pub fn get_scalar<N>(&self, name: &N) -> Option<&str>
    where
        String: Borrow<N>,
        N: Hash + Eq + ?Sized,
    {
        fn inner(var: &Variable) -> Option<&str> {
            match var.value.as_ref()? {
                Scalar(value) => Some(value),
                Array(_) => None,
            }
        }
        inner(self.get(name)?)
    }

    /// Unsets a variable.
    ///
    /// If successful, the return value is the previous value. If the specified
    /// variable is read-only, this function fails with [`UnsetError`].
    ///
    /// The behavior of unsetting depends on the `scope`:
    ///
    /// - If the scope is `Global`, this function removes the variable from all
    ///   contexts.
    /// - If the scope is `Local`, this function removes the variable from the
    ///   topmost [regular] context and any [volatile] context above it.
    /// - If the scope is `Volatile`, this function removes the variable from
    ///   any [volatile] context above the topmost [regular] context.
    ///
    /// In any case, this function may remove a variable from more than one
    /// context, in which case the return value is the value in the topmost
    /// context. If any of the removed variables is read-only, this function
    /// fails with [`UnsetError`] and does not remove any variable.
    ///
    /// You cannot modify positional parameters using this function.
    /// See [`positional_params_mut`](Self::positional_params_mut).
    ///
    /// [regular]: Context::Regular
    /// [volatile]: Context::Volatile
    pub fn unset<'a>(
        &'a mut self,
        name: &'a str,
        scope: Scope,
    ) -> Result<Option<Variable>, UnsetError<'a>> {
        let Some(stack) = self.all_variables.get_mut(name) else {
            return Ok(None);
        };

        // From which context should we unset?
        let index = Self::index_of_context(scope, &self.contexts);

        // Return an error if the variable is read-only.
        // Unfortunately, this code fragment does not compile because the
        // current Rust borrow checker is not smart enough.
        // TODO Uncomment this code when the borrow checker is improved
        // if let Some(read_only_location) = stack[index..]
        //     .iter()
        //     .filter_map(|vic| vic.variable.read_only_location.as_ref())
        //     .next_back()
        // {
        //     return Err(UnsetError {
        //         name,
        //         read_only_location,
        //     });
        // }
        if let Some(read_only_position) = stack[index..]
            .iter()
            .rposition(|vic| vic.variable.is_read_only())
        {
            let read_only_index = index + read_only_position;
            let read_only_location = &stack[read_only_index].variable.read_only_location;
            return Err(UnsetError {
                name,
                read_only_location: read_only_location.as_ref().unwrap(),
            });
        }

        Ok(stack.drain(index..).next_back().map(|vic| vic.variable))
    }

    /// Returns an iterator of variables.
    ///
    /// The `scope` parameter chooses variables returned by the iterator:
    ///
    /// - `Global`: all variables
    /// - `Local`: variables in the topmost [regular] context or above.
    /// - `Volatile`: variables above the topmost [regular] context
    ///
    /// In all cases, the iterator ignores variables hidden by another.
    ///
    /// The order of iterated variables is unspecified.
    ///
    /// [regular]: Context::Regular
    pub fn iter(&self, scope: Scope) -> Iter {
        Iter {
            inner: self.all_variables.iter(),
            min_context_index: Self::index_of_context(scope, &self.contexts),
        }
    }

    /// Returns environment variables in a new vector of C string.
    #[must_use]
    pub fn env_c_strings(&self) -> Vec<CString> {
        self.all_variables
            .iter()
            .filter_map(|(name, vars)| {
                let var = &vars.last()?.variable;
                let value = var.value.as_ref().filter(|_| var.is_exported)?;
                let mut result = name.clone();
                result.push('=');
                match value {
                    Scalar(value) => result.push_str(value),
                    Array(values) => write!(result, "{}", values.iter().format(":")).ok()?,
                }
                // TODO return something rather than dropping null-containing strings
                CString::new(result).ok()
            })
            .collect()
    }

    /// Imports environment variables from an iterator.
    ///
    /// The argument iterator must yield name-value pairs. This function assigns
    /// the values to the variable set, overwriting existing variables. The
    /// variables are exported.
    ///
    /// If an assignment fails because of an existing read-only variable, this
    /// function ignores the error and continues to the next assignment.
    pub fn extend_env<I, K, V>(&mut self, vars: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (name, value) in vars {
            let mut var = self.get_or_new(name, Scope::Global);
            if var.assign(value.into(), None).is_ok() {
                var.export(true)
            }
        }
    }

    /// Initializes default variables.
    ///
    /// This function assigns the following variables to `self`:
    ///
    /// - `IFS=' \t\n'`
    /// - `OPTIND=1`
    /// - `PS1='$ '`
    /// - `PS2='> '`
    /// - `PS4='+ '`
    /// - `LINENO` (with no value, but has its `quirk` set to [`Quirk::LineNumber`])
    ///
    /// The following variables are not assigned by this function as their
    /// values cannot be determined independently:
    ///
    /// - `PPID`
    /// - `PWD`
    ///
    /// This function ignores any assignment errors.
    pub fn init(&mut self) {
        const VARIABLES: &[(&str, &str)] = &[
            (IFS, IFS_INITIAL_VALUE),
            (OPTIND, OPTIND_INITIAL_VALUE),
            (PS1, PS1_INITIAL_VALUE_NON_ROOT),
            (PS2, PS2_INITIAL_VALUE),
            (PS4, PS4_INITIAL_VALUE),
        ];
        for &(name, value) in VARIABLES {
            self.get_or_new(name, Scope::Global)
                .assign(value, None)
                .ok();
        }

        self.get_or_new(LINENO, Scope::Global)
            .set_quirk(Some(Quirk::LineNumber))
    }

    /// Returns a reference to the positional parameters.
    ///
    /// Every regular context starts with an empty array of positional
    /// parameters, and volatile contexts cannot have positional parameters.
    /// This function returns a reference to the positional parameters of the
    /// topmost regular context.
    ///
    /// See also [`positional_params_mut`](Self::positional_params_mut).
    #[must_use]
    pub fn positional_params(&self) -> &PositionalParams {
        self.contexts
            .iter()
            .rev()
            .find_map(|context| match context {
                Context::Regular { positional_params } => Some(positional_params),
                Context::Volatile => None,
            })
            .expect("base context has gone")
    }

    /// Returns a mutable reference to the positional parameters.
    ///
    /// Every regular context starts with an empty array of positional
    /// parameters, and volatile contexts cannot have positional parameters.
    /// This function returns a reference to the positional parameters of the
    /// topmost regular context.
    #[must_use]
    pub fn positional_params_mut(&mut self) -> &mut PositionalParams {
        self.contexts
            .iter_mut()
            .rev()
            .find_map(|context| match context {
                Context::Regular { positional_params } => Some(positional_params),
                Context::Volatile => None,
            })
            .expect("base context has gone")
    }

    fn push_context_impl(&mut self, context: Context) {
        self.contexts.push(context);
    }

    fn pop_context_impl(&mut self) {
        debug_assert!(!self.contexts.is_empty());
        assert_ne!(self.contexts.len(), 1, "cannot pop the base context");
        self.contexts.pop();
        self.all_variables.retain(|_, stack| {
            if let Some(vic) = stack.last() {
                if vic.context_index >= self.contexts.len() {
                    stack.pop();
                }
            }
            !stack.is_empty()
        })
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a str, &'a Variable);

    fn next(&mut self) -> Option<(&'a str, &'a Variable)> {
        loop {
            let next = self.inner.next()?;
            if let Some(variable) = next.1.last() {
                if variable.context_index >= self.min_context_index {
                    return Some((next.0, &variable.variable));
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_min, max) = self.inner.size_hint();
        (0, max)
    }
}

impl FusedIterator for Iter<'_> {}

mod guard;

pub use self::guard::{ContextGuard, EnvContextGuard};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_variable_in_global_scope() {
        let mut set = VariableSet::new();
        set.push_context_impl(Context::default());
        set.push_context_impl(Context::Volatile);

        let mut var = set.get_or_new("foo", Scope::Global);

        assert_eq!(*var, Variable::default());
        var.assign("VALUE", None).unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        // The global variable still exists.
        assert_eq!(set.get("foo").unwrap().value, Some("VALUE".into()));
    }

    #[test]
    fn existing_variable_in_global_scope() {
        let mut set = VariableSet::new();
        let mut var = set.get_or_new("foo", Scope::Global);
        var.assign("ONE", None).unwrap();
        set.push_context_impl(Context::default());
        set.push_context_impl(Context::Volatile);

        let mut var = set.get_or_new("foo", Scope::Global);

        assert_eq!(var.value, Some("ONE".into()));
        var.assign("TWO", Location::dummy("somewhere")).unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        // The updated global variable still exists.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some("TWO".into()));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("somewhere")),
        );
    }

    #[test]
    fn new_variable_in_local_scope() {
        // This test case creates two local variables in separate contexts.
        let mut set = VariableSet::new();
        set.push_context_impl(Context::default());

        let mut var = set.get_or_new("foo", Scope::Local);

        assert_eq!(*var, Variable::default());

        var.assign("OUTER", None).unwrap();
        set.push_context_impl(Context::default());
        set.push_context_impl(Context::Volatile);

        let mut var = set.get_or_new("foo", Scope::Local);

        assert_eq!(*var, Variable::default());
        var.assign("INNER", Location::dummy("location")).unwrap();
        set.assert_normalized();
        set.pop_context_impl(); // volatile
        assert_eq!(set.get("foo").unwrap().value, Some("INNER".into()));
        set.pop_context_impl(); // regular
        assert_eq!(set.get("foo").unwrap().value, Some("OUTER".into()));
        set.pop_context_impl(); // regular
        assert_eq!(set.get("foo"), None);
    }

    #[test]
    fn existing_variable_in_local_scope() {
        let mut set = VariableSet::new();
        set.push_context_impl(Context::default());
        let mut var = set.get_or_new("foo", Scope::Local);
        var.assign("OLD", None).unwrap();

        let mut var = set.get_or_new("foo", Scope::Local);

        assert_eq!(var.value, Some("OLD".into()));
        var.assign("NEW", None).unwrap();
        assert_eq!(set.get("foo").unwrap().value, Some("NEW".into()));
        set.assert_normalized();
        set.pop_context_impl();
        assert_eq!(set.get("foo"), None);
    }

    #[test]
    fn new_variable_in_volatile_scope() {
        let mut set = VariableSet::new();
        set.push_context_impl(Context::Volatile);
        set.push_context_impl(Context::Volatile);

        let mut var = set.get_or_new("foo", Scope::Volatile);

        assert_eq!(*var, Variable::default());
        var.assign("VOLATILE", None).unwrap();
        assert_eq!(set.get("foo").unwrap().value, Some("VOLATILE".into()));
        set.assert_normalized();
        set.pop_context_impl();
        assert_eq!(set.get("foo"), None);
    }

    #[test]
    fn cloning_existing_regular_variable_to_volatile_context() {
        let mut set = VariableSet::new();
        let mut var = set.get_or_new("foo", Scope::Global);
        var.assign("VALUE", None).unwrap();
        var.make_read_only(Location::dummy("read-only location"));
        let save_var = var.clone();
        set.push_context_impl(Context::Volatile);
        set.push_context_impl(Context::Volatile);

        let mut var = set.get_or_new("foo", Scope::Volatile);

        assert_eq!(*var, save_var);
        var.export(true);
        assert!(set.get("foo").unwrap().is_exported);
        set.assert_normalized();
        set.pop_context_impl();
        // The exported variable is a volatile clone of the global variable.
        // The global variable is still not exported.
        assert_eq!(set.get("foo"), Some(&save_var));
    }

    #[test]
    fn existing_variable_in_volatile_scope() {
        let mut set = VariableSet::new();
        set.push_context_impl(Context::Volatile);
        let mut var = set.get_or_new("foo", Scope::Volatile);
        var.assign("INITIAL", None).unwrap();

        let mut var = set.get_or_new("foo", Scope::Volatile);

        assert_eq!(var.value, Some("INITIAL".into()));
        var.assign(Value::array(["MODIFIED"]), Location::dummy("somewhere"))
            .unwrap();
        assert_eq!(
            set.get("foo").unwrap().value,
            Some(Value::array(["MODIFIED"])),
        );
        set.assert_normalized();
        set.pop_context_impl();
        assert_eq!(set.get("foo"), None);
    }

    #[test]
    fn lowering_volatile_variable_to_base_context() {
        let mut set = VariableSet::new();
        set.push_context_impl(Context::default());
        set.push_context_impl(Context::Volatile);
        let mut var = set.get_or_new("foo", Scope::Volatile);
        var.assign("DUMMY", None).unwrap();
        set.push_context_impl(Context::Volatile);
        let mut var = set.get_or_new("foo", Scope::Volatile);
        var.assign("VOLATILE", Location::dummy("anywhere")).unwrap();
        var.export(true);

        let mut var = set.get_or_new("foo", Scope::Global);

        assert_eq!(var.value, Some("VOLATILE".into()));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("anywhere")),
        );
        var.assign("NEW", Location::dummy("somewhere")).unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        set.pop_context_impl();
        // The value DUMMY is now gone.
        // The value VOLATILE has been overwritten by NEW.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some("NEW".into()));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("somewhere")),
        );
        // But it's still exported.
        assert!(var.is_exported);
    }

    #[test]
    fn lowering_volatile_variable_to_middle_regular_context() {
        let mut set = VariableSet::new();
        let mut var = set.get_or_new("foo", Scope::Local);
        var.assign("ONE", None).unwrap();
        set.push_context_impl(Context::default());
        let mut var = set.get_or_new("foo", Scope::Local);
        var.assign("TWO", None).unwrap();
        set.push_context_impl(Context::default());
        set.push_context_impl(Context::Volatile);
        let mut var = set.get_or_new("foo", Scope::Volatile);
        var.assign("VOLATILE", Location::dummy("anywhere")).unwrap();
        var.export(true);

        let mut var = set.get_or_new("foo", Scope::Global);

        assert_eq!(var.value, Some("VOLATILE".into()));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("anywhere")),
        );
        var.assign("NEW", Location::dummy("somewhere")).unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        // The value TWO has been overwritten by NEW.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some("NEW".into()));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("somewhere")),
        );
        // But it's still exported.
        assert!(var.is_exported);
        set.pop_context_impl();
        // The value ONE is still there.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some("ONE".into()));
    }

    #[test]
    fn lowering_volatile_variable_to_topmost_regular_context_without_existing_variable() {
        let mut set = VariableSet::new();
        set.push_context_impl(Context::default());
        set.push_context_impl(Context::default());
        set.push_context_impl(Context::Volatile);
        let mut var = set.get_or_new("foo", Scope::Volatile);
        var.assign("DUMMY", None).unwrap();
        set.push_context_impl(Context::Volatile);
        let mut var = set.get_or_new("foo", Scope::Volatile);
        var.assign("VOLATILE", Location::dummy("anywhere")).unwrap();
        var.export(true);

        let mut var = set.get_or_new("foo", Scope::Local);

        assert_eq!(var.value, Some("VOLATILE".into()));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("anywhere")),
        );
        var.assign("NEW", Location::dummy("somewhere")).unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        // The value DUMMY is now gone.
        // The value VOLATILE has been overwritten by NEW.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some("NEW".into()));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("somewhere")),
        );
        // But it's still exported.
        assert!(var.is_exported);
    }

    #[test]
    fn lowering_volatile_variable_to_topmost_regular_context_overwriting_existing_variable() {
        let mut set = VariableSet::new();
        set.push_context_impl(Context::default());
        set.push_context_impl(Context::default());
        let mut var = set.get_or_new("foo", Scope::Local);
        var.assign("OLD", None).unwrap();
        set.push_context_impl(Context::Volatile);
        let mut var = set.get_or_new("foo", Scope::Volatile);
        var.assign("DUMMY", None).unwrap();
        set.push_context_impl(Context::Volatile);
        let mut var = set.get_or_new("foo", Scope::Volatile);
        var.assign("VOLATILE", Location::dummy("first")).unwrap();
        var.export(true);
        set.push_context_impl(Context::Volatile);

        let mut var = set.get_or_new("foo", Scope::Local);

        assert_eq!(var.value, Some("VOLATILE".into()));
        assert_eq!(var.last_assigned_location, Some(Location::dummy("first")));
        var.assign("NEW", Location::dummy("second")).unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        set.pop_context_impl();
        // The value DUMMY is now gone.
        // The value OLD has been overwritten by NEW.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some("NEW".into()));
        assert_eq!(var.last_assigned_location, Some(Location::dummy("second")));
        // But it's still exported.
        assert!(var.is_exported);
    }

    #[test]
    #[should_panic(expected = "no volatile context to store the variable")]
    fn missing_volatile_context() {
        let mut set = VariableSet::new();
        set.get_or_new("foo", Scope::Volatile);
    }

    #[test]
    fn getting_variables_with_scopes() {
        let mut set = VariableSet::new();
        set.get_or_new("global", Scope::Global)
            .assign("G", None)
            .unwrap();
        set.push_context_impl(Context::default());
        set.get_or_new("local", Scope::Local)
            .assign("L", None)
            .unwrap();
        set.push_context_impl(Context::Volatile);
        set.get_or_new("volatile", Scope::Volatile)
            .assign("V", None)
            .unwrap();

        assert_eq!(
            set.get_scoped("global", Scope::Global),
            Some(&Variable::new("G")),
        );
        assert_eq!(set.get_scoped("global", Scope::Local), None);
        assert_eq!(set.get_scoped("global", Scope::Volatile), None);

        assert_eq!(
            set.get_scoped("local", Scope::Global),
            Some(&Variable::new("L"))
        );
        assert_eq!(
            set.get_scoped("local", Scope::Local),
            Some(&Variable::new("L"))
        );
        assert_eq!(set.get_scoped("local", Scope::Volatile), None);

        assert_eq!(
            set.get_scoped("volatile", Scope::Global),
            Some(&Variable::new("V"))
        );
        assert_eq!(
            set.get_scoped("volatile", Scope::Local),
            Some(&Variable::new("V"))
        );
        assert_eq!(
            set.get_scoped("volatile", Scope::Volatile),
            Some(&Variable::new("V"))
        );
    }

    #[test]
    fn unsetting_nonexisting_variable() {
        let mut variables = VariableSet::new();
        let result = variables.unset("", Scope::Global).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn unsetting_variable_with_one_context() {
        let mut variables = VariableSet::new();
        variables
            .get_or_new("foo", Scope::Global)
            .assign("X", None)
            .unwrap();

        let result = variables.unset("foo", Scope::Global).unwrap();
        assert_eq!(result, Some(Variable::new("X")));
        assert_eq!(variables.get("foo"), None);
    }

    #[test]
    fn unsetting_variables_from_all_contexts() {
        let mut variables = VariableSet::new();
        variables
            .get_or_new("foo", Scope::Global)
            .assign("X", None)
            .unwrap();
        variables.push_context_impl(Context::default());
        variables
            .get_or_new("foo", Scope::Local)
            .assign("Y", None)
            .unwrap();
        variables.push_context_impl(Context::Volatile);
        variables
            .get_or_new("foo", Scope::Volatile)
            .assign("Z", None)
            .unwrap();

        let result = variables.unset("foo", Scope::Global).unwrap();
        assert_eq!(result, Some(Variable::new("Z")));
        assert_eq!(variables.get("foo"), None);
    }

    #[test]
    fn unsetting_variable_from_local_context() {
        let mut variables = VariableSet::new();
        variables
            .get_or_new("foo", Scope::Global)
            .assign("A", None)
            .unwrap();
        variables.push_context_impl(Context::default());
        // Non-local read-only variable does not prevent unsetting
        let mut readonly_foo = variables.get_or_new("foo", Scope::Local);
        readonly_foo.assign("B", None).unwrap();
        readonly_foo.make_read_only(Location::dummy("dummy"));
        let readonly_foo = readonly_foo.clone();
        variables.push_context_impl(Context::default());
        variables
            .get_or_new("foo", Scope::Local)
            .assign("C", None)
            .unwrap();
        variables.push_context_impl(Context::Volatile);
        variables
            .get_or_new("foo", Scope::Volatile)
            .assign("D", None)
            .unwrap();

        let result = variables.unset("foo", Scope::Local).unwrap();
        assert_eq!(result, Some(Variable::new("D")));
        assert_eq!(variables.get("foo"), Some(&readonly_foo));
    }

    #[test]
    fn unsetting_nonexisting_variable_in_local_context() {
        let mut variables = VariableSet::new();
        variables
            .get_or_new("foo", Scope::Global)
            .assign("A", None)
            .unwrap();
        variables.push_context_impl(Context::default());

        let result = variables.unset("foo", Scope::Local).unwrap();
        assert_eq!(result, None);
        assert_eq!(variables.get("foo"), Some(&Variable::new("A")));
    }

    #[test]
    fn unsetting_variable_from_volatile_context() {
        let mut variables = VariableSet::new();
        variables
            .get_or_new("foo", Scope::Global)
            .assign("A", None)
            .unwrap();
        variables.push_context_impl(Context::default());
        variables
            .get_or_new("foo", Scope::Local)
            .assign("B", None)
            .unwrap();
        variables.push_context_impl(Context::Volatile);
        variables
            .get_or_new("foo", Scope::Volatile)
            .assign("C", None)
            .unwrap();
        variables.push_context_impl(Context::Volatile);
        variables
            .get_or_new("foo", Scope::Volatile)
            .assign("D", None)
            .unwrap();

        let result = variables.unset("foo", Scope::Volatile).unwrap();
        assert_eq!(result, Some(Variable::new("D")));
        assert_eq!(variables.get("foo"), Some(&Variable::new("B")));
    }

    #[test]
    fn unsetting_nonexisting_variable_in_volatile_context() {
        let mut variables = VariableSet::new();
        variables
            .get_or_new("foo", Scope::Global)
            .assign("A", None)
            .unwrap();
        variables.push_context_impl(Context::Volatile);

        let result = variables.unset("foo", Scope::Volatile).unwrap();
        assert_eq!(result, None);
        assert_eq!(variables.get("foo"), Some(&Variable::new("A")));
    }

    #[test]
    fn unsetting_readonly_variable() {
        let read_only_location = &Location::dummy("read-only");
        let mut variables = VariableSet::new();
        let mut foo = variables.get_or_new("foo", Scope::Global);
        foo.assign("A", None).unwrap();
        variables.push_context_impl(Context::default());
        let mut foo = variables.get_or_new("foo", Scope::Local);
        foo.assign("B", None).unwrap();
        foo.make_read_only(Location::dummy("dummy"));
        variables.push_context_impl(Context::default());
        let mut foo = variables.get_or_new("foo", Scope::Local);
        foo.assign("C", None).unwrap();
        foo.make_read_only(read_only_location.clone());
        variables.push_context_impl(Context::default());
        let mut foo = variables.get_or_new("foo", Scope::Local);
        foo.assign("D", None).unwrap();

        let error = variables.unset("foo", Scope::Global).unwrap_err();
        assert_eq!(
            error,
            UnsetError {
                name: "foo",
                read_only_location
            }
        );
        assert_eq!(variables.get("foo"), Some(&Variable::new("D")));
    }

    #[test]
    #[should_panic(expected = "cannot pop the base context")]
    fn cannot_pop_base_context() {
        let mut variables = VariableSet::new();
        variables.pop_context_impl();
    }

    fn test_iter<F: FnOnce(&VariableSet)>(f: F) {
        let mut set = VariableSet::new();

        let mut var = set.get_or_new("global", Scope::Global);
        var.assign("global value", None).unwrap();
        var.export(true);
        let mut var = set.get_or_new("local", Scope::Global);
        var.assign("hidden value", None).unwrap();

        let mut set = set.push_context(Context::default());

        let mut var = set.get_or_new("local", Scope::Local);
        var.assign("visible value", None).unwrap();
        let mut var = set.get_or_new("volatile", Scope::Local);
        var.assign("hidden value", None).unwrap();

        let mut set = set.push_context(Context::Volatile);

        let mut var = set.get_or_new("volatile", Scope::Volatile);
        var.assign("volatile value", None).unwrap();

        f(&mut set);
    }

    #[test]
    fn iter_global() {
        test_iter(|set| {
            let mut v: Vec<_> = set.iter(Scope::Global).collect();
            v.sort_unstable_by_key(|&(name, _)| name);
            assert_eq!(
                v,
                [
                    ("global", &Variable::new("global value").export()),
                    ("local", &Variable::new("visible value")),
                    ("volatile", &Variable::new("volatile value"))
                ]
            );
        })
    }

    #[test]
    fn iter_local() {
        test_iter(|set| {
            let mut v: Vec<_> = set.iter(Scope::Local).collect();
            v.sort_unstable_by_key(|&(name, _)| name);
            assert_eq!(
                v,
                [
                    ("local", &Variable::new("visible value")),
                    ("volatile", &Variable::new("volatile value"))
                ]
            );
        })
    }

    #[test]
    fn iter_volatile() {
        test_iter(|set| {
            let mut v: Vec<_> = set.iter(Scope::Volatile).collect();
            v.sort_unstable_by_key(|&(name, _)| name);
            assert_eq!(v, [("volatile", &Variable::new("volatile value"))]);
        })
    }

    #[test]
    fn iter_size_hint() {
        test_iter(|set| {
            assert_eq!(set.iter(Scope::Global).size_hint(), (0, Some(3)));
            assert_eq!(set.iter(Scope::Local).size_hint(), (0, Some(3)));
            assert_eq!(set.iter(Scope::Volatile).size_hint(), (0, Some(3)));
        })
    }

    #[test]
    fn env_c_strings() {
        let mut variables = VariableSet::new();
        assert_eq!(&variables.env_c_strings(), &[]);

        let mut var = variables.get_or_new("foo", Scope::Global);
        var.assign("FOO", None).unwrap();
        var.export(true);
        let mut var = variables.get_or_new("bar", Scope::Global);
        var.assign(Value::array(["BAR"]), None).unwrap();
        var.export(true);
        let mut var = variables.get_or_new("baz", Scope::Global);
        var.assign(Value::array(["1", "two", "3"]), None).unwrap();
        var.export(true);
        let mut var = variables.get_or_new("null", Scope::Global);
        var.assign("not exported", None).unwrap();
        variables.get_or_new("none", Scope::Global);

        let mut ss = variables.env_c_strings();
        ss.sort_unstable();
        assert_eq!(
            &ss,
            &[
                c"bar=BAR".to_owned(),
                c"baz=1:two:3".to_owned(),
                c"foo=FOO".to_owned()
            ]
        );
    }

    #[test]
    fn extend_env() {
        let mut variables = VariableSet::new();

        variables.extend_env([("foo", "FOO"), ("bar", "OK")]);

        let foo = variables.get("foo").unwrap();
        assert_eq!(foo.value, Some("FOO".into()));
        assert!(foo.is_exported);
        let bar = variables.get("bar").unwrap();
        assert_eq!(bar.value, Some("OK".into()));
        assert!(bar.is_exported);
    }

    #[test]
    fn init_lineno() {
        let mut variables = VariableSet::new();
        variables.init();
        let v = variables.get(LINENO).unwrap();
        assert_eq!(v.value, None);
        assert_eq!(v.quirk, Some(Quirk::LineNumber));
        assert_eq!(v.last_assigned_location, None);
        assert!(!v.is_exported);
        assert_eq!(v.read_only_location, None);
    }

    #[test]
    fn positional_params_in_base_context() {
        let mut variables = VariableSet::new();
        assert_eq!(variables.positional_params().values, [] as [String; 0]);

        let params = variables.positional_params_mut();
        params.values.push("foo".to_string());
        params.values.push("bar".to_string());

        assert_eq!(
            variables.positional_params().values,
            ["foo".to_string(), "bar".to_string()],
        );
    }

    #[test]
    fn positional_params_in_second_regular_context() {
        let mut variables = VariableSet::new();
        variables.push_context_impl(Context::default());
        assert_eq!(variables.positional_params().values, [] as [String; 0]);

        let params = variables.positional_params_mut();
        params.values.push("1".to_string());

        assert_eq!(variables.positional_params().values, ["1".to_string()]);
    }

    #[test]
    fn getting_positional_params_in_volatile_context() {
        let mut variables = VariableSet::new();

        let params = variables.positional_params_mut();
        params.values.push("a".to_string());
        params.values.push("b".to_string());
        params.values.push("c".to_string());

        variables.push_context_impl(Context::Volatile);
        assert_eq!(
            variables.positional_params().values,
            ["a".to_string(), "b".to_string(), "c".to_string()],
        );
    }

    #[test]
    fn setting_positional_params_in_volatile_context() {
        let mut variables = VariableSet::new();
        variables.push_context_impl(Context::Volatile);

        let params = variables.positional_params_mut();
        params.values.push("x".to_string());

        variables.pop_context_impl();
        assert_eq!(variables.positional_params().values, ["x".to_string()]);
    }
}
