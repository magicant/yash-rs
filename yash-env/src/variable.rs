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

//! Type definitions for shell variables.
//!
//! A [`VariableSet`] is a stack of contexts, and a _context_ is a map of
//! name-variable pairs. The `VariableSet` has a _base context_ with the same
//! lifetime as the `VariableSet` itself. Additional contexts can be added
//! (pushed) and removed (popped) on a last-in-first-out basis.
//!
//! You can define any number of [`Variable`]s in a context.
//! A new context is empty when pushed to the variable set.
//! You can pop a context regardless of whether it is empty or not;
//! all the variables in the popped context are removed as well.
//!
//! Variables in a context hide those with the same name in lower contexts. You
//! cannot access such hidden variables until removing the hiding variable from
//! the upper context.
//!
//! Each regular context has a special array variable called positional
//! parameters. Because it does not have a name as a variable, you need to use
//! dedicated methods for accessing it.
//! See [`VariableSet::positional_params`] and its [mut
//! variant](VariableSet::positional_params_mut).
//!
//! This module provides guards to ensure contexts are pushed and popped
//! correctly. The push function returns a guard that will pop the context when
//! dropped. Implementing `Deref` and `DerefMut`, the guard allows access to the
//! borrowed `VariableSet` or `Env`. [`VariableSet::push_context`] returns a
//! [`ContextGuard`] that allows re-borrowing the `VariableSet`.
//! [`Env::push_context`] returns a [`EnvContextGuard`] that implements
//! `DerefMut<Target = Env>`.

#[cfg(doc)]
use crate::Env;
use itertools::Itertools;
use std::borrow::Borrow;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::HashMap;
use std::ffi::CString;
use std::fmt::Write;
use std::hash::Hash;
use std::iter::FusedIterator;
use thiserror::Error;
use yash_syntax::source::Location;

mod value;

pub use self::value::{Array, Scalar, Value};

mod quirk;

pub use self::quirk::Expansion;
pub use self::quirk::Quirk;

mod main;

pub use self::main::AssignError as NewAssignError; // TODO Remove this alias
pub use self::main::Variable;
pub use self::main::VariableRefMut;

#[derive(Clone, Debug, Eq, PartialEq)]
struct VariableInContext {
    variable: Variable,
    context_index: usize,
}

/// Type of a context.
///
/// The context type affects the behavior of variable
/// [assignment](VariableSet::assign).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextType {
    /// Context for normal assignments.
    ///
    /// The base context is a regular context. The context for a function's
    /// local assignment is also regular.
    Regular,

    /// Context for temporary assignments.
    ///
    /// A volatile context is used for holding temporary variables when
    /// executing a built-in or function.
    Volatile,
}

/// Variable context.
///
/// Variables defined in the context are not stored in this struct.
/// See `VariableSet::all_variables`.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Context {
    /// Context type.
    r#type: ContextType,

    /// Positional parameters.
    ///
    /// This variable is very special:
    ///
    /// - Its value is always an `Array`.
    /// - It is never exported nor read-only.
    positional_params: Variable,
}

impl Context {
    fn new(r#type: ContextType) -> Self {
        Context {
            r#type,
            positional_params: Variable::new_empty_array(),
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
            contexts: vec![Context::new(ContextType::Regular)],
        }
    }
}

/// Choice of a context to which a variable is assigned or searched for.
///
/// For the meaning of the variants of this enum, see the docs for the functions
/// that use it: [`VariableSet::assign`] and [`VariableSet::iter`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    Global,
    Local,
    Volatile,
}

/// Error that occurs when assigning to an existing read-only variable.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cannot assign to read-only variable `{name}`")]
pub struct AssignError {
    /// Variable name.
    pub name: String,
    /// Location where the existing variable was made read-only.
    pub read_only_location: Location,
    /// New variable that was tried to assign.
    pub new_value: Variable,
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
    /// _visible_ and returned.
    ///
    /// You cannot retrieve positional parameters using this function.
    /// See [`positional_params`](Self::positional_params).
    #[must_use]
    pub fn get<N: ?Sized>(&self, name: &N) -> Option<&Variable>
    where
        String: Borrow<N>,
        N: Hash + Eq,
    {
        Some(&self.all_variables.get(name)?.last()?.variable)
    }

    /// Assigns a variable.
    ///
    /// If successful, the return value is the previous value. If there is an
    /// existing read-only value, the assignment fails unless the new variable
    /// is a local variable that hides the read-only.
    ///
    /// If an existing same-named variable is exported, this function also
    /// exports the new value. Otherwise, the value is assigned without
    /// modification. To apply the `AllExport` [shell
    /// option](crate::option::Option), you should prefer the
    /// [`Env::assign_variable`] function rather than calling this function
    /// directly.
    ///
    /// The behavior of assignment depends on the `scope`:
    ///
    /// - If the scope is `Global`, the assignment overwrites a visible existing
    ///   variable in a regular context, if any. Existing
    ///   [volatile](ContextType::Volatile) variables are removed to make the
    ///   target variable visible. If there is no variable, a new variable is
    ///   inserted to the base context.
    /// - If the scope is `Local`, the variable is added to the topmost
    ///   [regular](ContextType::Regular) context, which may overwrite an
    ///   existing variable in the target context and hide variables in lower
    ///   contexts.  Existing [volatile](ContextType::Volatile) variables are
    ///   removed to make the target variable visible.
    /// - A `Volatile`-scoped assignment requires the topmost context to be
    ///   [volatile](ContextType::Volatile); otherwise, the assignment would
    ///   **panic!** The variable is assigned to the topmost context.
    ///
    /// Note that this function does not return variables that it removed from
    /// volatile contexts to make the assigned variable visible.
    ///
    /// The current implementation assumes that variables in volatile contexts
    /// are not read-only.
    ///
    /// You cannot modify positional parameters using this function.
    /// See [`positional_params_mut`](Self::positional_params_mut).
    pub fn assign(
        &mut self,
        scope: Scope,
        name: String,
        mut value: Variable,
    ) -> Result<Option<Variable>, AssignError> {
        use std::collections::hash_map::Entry;
        // TODO Can we avoid cloning the name here?
        let stack = match self.all_variables.entry(name.clone()) {
            Entry::Vacant(vacant) => vacant.insert(Vec::new()),
            Entry::Occupied(occupied) => occupied.into_mut(),
        };

        // Volatile assignment cannot hide a read-only variable.
        if scope == Scope::Volatile {
            if let Some(vic) = stack.last() {
                if let Some(location) = &vic.variable.read_only_location {
                    return Err(AssignError {
                        name,
                        read_only_location: location.clone(),
                        new_value: value,
                    });
                }
            }
        }

        // To which context should we assign?
        let contexts = &self.contexts;
        let context_index = match scope {
            Scope::Global => stack
                .iter()
                .filter(|vic| contexts[vic.context_index].r#type != ContextType::Volatile)
                .next_back()
                .map_or(0, |vic| vic.context_index),
            Scope::Local => contexts
                .iter()
                .rposition(|c| c.r#type == ContextType::Regular)
                .expect("base context has gone"),
            Scope::Volatile => {
                let top_context = contexts.last().expect("base context has gone");
                assert_eq!(
                    top_context.r#type,
                    ContextType::Volatile,
                    "volatile scope assignment requires volatile context"
                );
                contexts.len() - 1
            }
        };

        // Remove volatile variables.
        while stack
            .last()
            .filter(|vic| vic.context_index > context_index)
            .is_some()
        {
            stack.pop();
        }

        // Do the assignment.
        let existing = stack
            .last_mut()
            .filter(|vic| vic.context_index == context_index)
            .map(|vic| &mut vic.variable);
        if let Some(existing) = existing {
            if let Some(location) = &existing.read_only_location {
                return Err(AssignError {
                    name,
                    read_only_location: location.clone(),
                    new_value: value,
                });
            }

            value.is_exported |= existing.is_exported;
            Ok(Some(std::mem::replace(existing, value)))
        } else {
            stack.push(VariableInContext {
                variable: value,
                context_index,
            });
            Ok(None)
        }
    }

    /// Computes the index of the topmost regular context.
    fn index_of_topmost_regular_context(contexts: &[Context]) -> usize {
        contexts
            .iter()
            .rposition(|context| context.r#type == ContextType::Regular)
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
    /// You cannot modify positional parameters using this function.
    /// See [`positional_params_mut`](Self::positional_params_mut).
    ///
    /// [regular]: ContextType::Regular
    /// [volatile]: ContextType::Volatile
    pub fn get_or_new(&mut self, name: String, scope: Scope) -> VariableRefMut {
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
                    match self.contexts[var.context_index].r#type {
                        ContextType::Regular => {
                            if let Some(removed_volatile_variable) = removed_volatile_variable {
                                var.variable = removed_volatile_variable;
                            }
                            break 'branch;
                        }
                        ContextType::Volatile => {
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
                    self.contexts[context_index].r#type,
                    ContextType::Volatile,
                    "no volatile context to store the variable"
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
    /// [regular]: ContextType::Regular
    /// [volatile]: ContextType::Volatile
    pub fn unset<'a>(
        &'a mut self,
        scope: Scope,
        name: &'a str,
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
    /// - `Local`: variables in the topmost [regular](ContextType::Regular)
    ///   context or above.
    /// - `Volatile`: variables above the topmost
    ///   [regular](ContextType::Regular) context
    ///
    /// In all cases, the iterator ignores variables hidden by another.
    ///
    /// The order of iterated variables is unspecified.
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
            self.assign(Scope::Global, name.into(), Variable::new(value).export())
                .ok();
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
            ("IFS", " \t\n"),
            ("OPTIND", "1"),
            ("PS1", "$ "),
            ("PS2", "> "),
            ("PS4", "+ "),
        ];
        for &(name, value) in VARIABLES {
            let _ = self.assign(Scope::Global, name.to_owned(), Variable::new(value));
        }

        let v = Variable {
            quirk: Some(Quirk::LineNumber),
            ..Default::default()
        };
        let _ = self.assign(Scope::Global, "LINENO".to_string(), v);
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
    pub fn positional_params(&self) -> &Variable {
        let index = Self::index_of_topmost_regular_context(&self.contexts);
        &self.contexts[index].positional_params
    }

    /// Returns a mutable reference to the positional parameters.
    ///
    /// Although positional parameters are not considered a variable in the
    /// POSIX standard, we implement them as an anonymous array variable. It is
    /// the caller's responsibility to keep the variable in a correct state:
    ///
    /// - The variable value should be an array. Not a scalar.
    /// - The variable should not be exported nor made read-only.
    ///
    /// The `VariableSet` does not check if these rules are maintained.
    ///
    /// Every regular context starts with an empty array of positional
    /// parameters, and volatile contexts cannot have positional parameters.
    /// This function returns a reference to the positional parameters of the
    /// topmost regular context.
    #[must_use]
    pub fn positional_params_mut(&mut self) -> &mut Variable {
        let index = Self::index_of_topmost_regular_context(&self.contexts);
        &mut self.contexts[index].positional_params
    }

    fn push_context_impl(&mut self, context_type: ContextType) {
        self.contexts.push(Context::new(context_type));
    }

    fn pop_context_impl(&mut self) {
        debug_assert!(!self.contexts.is_empty());
        assert_ne!(self.contexts.len(), 1, "cannot pop the base context");
        self.contexts.pop();
        // TODO Use complementary stack of hash tables to avoid scanning the
        // whole `self.all_variables`
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
    use assert_matches::assert_matches;

    #[test]
    fn assign_new_variable_and_get() {
        let mut variables = VariableSet::new();
        let variable = Variable::new("my value").make_read_only(Location::dummy("dummy"));
        let result = variables
            .assign(Scope::Global, "foo".to_string(), variable.clone())
            .unwrap();
        assert_eq!(result, None);
        assert_eq!(variables.get("foo"), Some(&variable));
    }

    #[test]
    fn reassign_variable_and_get() {
        let mut variables = VariableSet::new();
        let v1 = Variable::new("my value").set_assigned_location(Location::dummy("dummy"));
        variables
            .assign(Scope::Global, "foo".to_string(), v1.clone())
            .unwrap();

        let v2 = Variable::new("your value").make_read_only(Location::dummy("something"));
        let result = variables
            .assign(Scope::Global, "foo".to_string(), v2.clone())
            .unwrap();
        assert_eq!(result, Some(v1));
        assert_eq!(variables.get("foo"), Some(&v2));
    }

    #[test]
    fn assign_to_read_only_variable() {
        let mut variables = VariableSet::new();
        let read_only_location = Location::dummy("read-only");
        let v1 = Variable::new("my value").make_read_only(read_only_location.clone());
        variables
            .assign(Scope::Global, "x".to_string(), v1.clone())
            .unwrap();

        let v2 = Variable::new("your value").make_read_only(Location::dummy("something"));
        let error = variables
            .assign(Scope::Global, "x".to_string(), v2.clone())
            .unwrap_err();
        assert_eq!(error.name, "x");
        assert_eq!(error.read_only_location, read_only_location);
        assert_eq!(error.new_value, v2);
        assert_eq!(variables.get("x"), Some(&v1));
    }

    #[test]
    fn assign_global() {
        let mut variables = VariableSet::new();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new(""))
            .unwrap();
        variables.pop_context_impl();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("")));
    }

    #[test]
    fn assign_local() {
        let mut variables = VariableSet::new();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new(""))
            .unwrap();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("")));
    }

    #[test]
    fn popping_context_removes_variables() {
        let mut variables = VariableSet::new();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new(""))
            .unwrap();
        variables.pop_context_impl();
        assert_eq!(variables.get("foo"), None);
    }

    #[test]
    fn reassign_global_non_base_context() {
        let mut variables = VariableSet::new();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new("a"))
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new("b"))
            .unwrap();
        variables.pop_context_impl();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("b")));
        variables.pop_context_impl();
        assert_eq!(variables.get("foo"), None);
    }

    #[test]
    fn variable_in_upper_context_hides_lower_variables() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new("0"))
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new("1"))
            .unwrap();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("1")));
    }

    #[test]
    fn variable_is_visible_again_after_popping_upper_variables() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new("0"))
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new("1"))
            .unwrap();
        variables.pop_context_impl();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("0")));
    }

    #[test]
    fn volatile_assignment_new() {
        let mut variables = VariableSet::new();
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("0"))
            .unwrap();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("0")));
    }

    #[test]
    fn volatile_assignment_hides_existing_variable() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new("0"))
            .unwrap();
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("1"))
            .unwrap();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("1")));
        variables.pop_context_impl();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("0")));
    }

    #[test]
    fn volatile_assignment_fails_with_existing_read_only_variable() {
        let mut variables = VariableSet::new();
        let read_only_location = Location::dummy("ROL");
        let read_only = Variable::new("0").make_read_only(read_only_location.clone());
        variables
            .assign(Scope::Global, "foo".to_string(), read_only)
            .unwrap();
        variables.push_context_impl(ContextType::Volatile);
        let error = variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("1"))
            .unwrap_err();
        assert_eq!(error.name, "foo");
        assert_eq!(error.read_only_location, read_only_location);
        assert_eq!(error.new_value.value, Some(Value::scalar("1")));
    }

    #[test]
    #[should_panic(expected = "volatile scope assignment requires volatile context")]
    fn volatile_assignment_panics_without_volatile_context() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("0"))
            .unwrap();
    }

    #[test]
    fn global_assignment_pops_existing_volatile_variables() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new("0"))
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("1"))
            .unwrap();
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("2"))
            .unwrap();
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new("9"))
            .unwrap();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("9")));
        variables.pop_context_impl();
        variables.pop_context_impl();
        variables.pop_context_impl();
        variables.pop_context_impl();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("9")));
    }

    #[test]
    fn local_assignment_pops_existing_volatile_variables() {
        let mut variables = VariableSet::new();
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("0"))
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("1"))
            .unwrap();
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("2"))
            .unwrap();
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new("9"))
            .unwrap();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("9")));
        variables.pop_context_impl();
        variables.pop_context_impl();
        variables.pop_context_impl();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("9")));
        variables.pop_context_impl();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("0")));
        variables.pop_context_impl();
        assert_eq!(variables.get("foo"), None);
    }

    #[test]
    fn new_variable_in_global_scope() {
        let mut set = VariableSet::new();
        set.push_context_impl(ContextType::Regular);
        set.push_context_impl(ContextType::Volatile);

        let mut var = set.get_or_new("foo".into(), Scope::Global);

        assert_eq!(*var, Variable::default());
        var.assign(Value::scalar("VALUE"), None).unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        // The global variable still exists.
        assert_eq!(set.get("foo").unwrap().value, Some(Value::scalar("VALUE")));
    }

    #[test]
    fn existing_variable_in_global_scope() {
        let mut set = VariableSet::new();
        let mut var = set.get_or_new("foo".into(), Scope::Global);
        var.assign(Value::scalar("ONE"), None).unwrap();
        set.push_context_impl(ContextType::Regular);
        set.push_context_impl(ContextType::Volatile);

        let mut var = set.get_or_new("foo".into(), Scope::Global);

        assert_eq!(var.value, Some(Value::scalar("ONE")));
        var.assign(Value::scalar("TWO"), Some(Location::dummy("somewhere")))
            .unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        // The updated global variable still exists.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some(Value::scalar("TWO")));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("somewhere")),
        );
    }

    #[test]
    fn new_variable_in_local_scope() {
        // This test case creates two local variables in separate contexts.
        let mut set = VariableSet::new();
        set.push_context_impl(ContextType::Regular);

        let mut var = set.get_or_new("foo".into(), Scope::Local);

        assert_eq!(*var, Variable::default());

        var.assign(Value::scalar("OUTER"), None).unwrap();
        set.push_context_impl(ContextType::Regular);
        set.push_context_impl(ContextType::Volatile);

        let mut var = set.get_or_new("foo".into(), Scope::Local);

        assert_eq!(*var, Variable::default());
        var.assign(Value::scalar("INNER"), Some(Location::dummy("location")))
            .unwrap();
        set.assert_normalized();
        set.pop_context_impl(); // volatile
        assert_eq!(set.get("foo").unwrap().value, Some(Value::scalar("INNER")));
        set.pop_context_impl(); // regular
        assert_eq!(set.get("foo").unwrap().value, Some(Value::scalar("OUTER")));
        set.pop_context_impl(); // regular
        assert_eq!(set.get("foo"), None);
    }

    #[test]
    fn existing_variable_in_local_scope() {
        let mut set = VariableSet::new();
        set.push_context_impl(ContextType::Regular);
        let mut var = set.get_or_new("foo".into(), Scope::Local);
        var.assign(Value::scalar("OLD"), None).unwrap();

        let mut var = set.get_or_new("foo".into(), Scope::Local);

        assert_eq!(var.value, Some(Value::scalar("OLD")));
        var.assign(Value::scalar("NEW"), None).unwrap();
        assert_eq!(set.get("foo").unwrap().value, Some(Value::scalar("NEW")));
        set.assert_normalized();
        set.pop_context_impl();
        assert_eq!(set.get("foo"), None);
    }

    #[test]
    fn new_variable_in_volatile_scope() {
        let mut set = VariableSet::new();
        set.push_context_impl(ContextType::Volatile);
        set.push_context_impl(ContextType::Volatile);

        let mut var = set.get_or_new("foo".into(), Scope::Volatile);

        assert_eq!(*var, Variable::default());
        var.assign(Value::scalar("VOLATILE"), None).unwrap();
        assert_eq!(
            set.get("foo").unwrap().value,
            Some(Value::scalar("VOLATILE")),
        );
        set.assert_normalized();
        set.pop_context_impl();
        assert_eq!(set.get("foo"), None);
    }

    #[test]
    fn cloning_existing_regular_variable_to_volatile_context() {
        let mut set = VariableSet::new();
        let mut var = set.get_or_new("foo".into(), Scope::Global);
        var.assign(Value::scalar("VALUE"), None).unwrap();
        var.make_read_only(Location::dummy("read-only location"));
        let save_var = var.clone();
        set.push_context_impl(ContextType::Volatile);
        set.push_context_impl(ContextType::Volatile);

        let mut var = set.get_or_new("foo".into(), Scope::Volatile);

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
        set.push_context_impl(ContextType::Volatile);
        let mut var = set.get_or_new("foo".into(), Scope::Volatile);
        var.assign(Value::scalar("INITIAL"), None).unwrap();

        let mut var = set.get_or_new("foo".into(), Scope::Volatile);

        assert_eq!(var.value, Some(Value::scalar("INITIAL")));
        var.assign(
            Value::array(["MODIFIED"]),
            Some(Location::dummy("somewhere")),
        )
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
        set.push_context_impl(ContextType::Regular);
        set.push_context_impl(ContextType::Volatile);
        let mut var = set.get_or_new("foo".into(), Scope::Volatile);
        var.assign(Value::scalar("DUMMY"), None).unwrap();
        set.push_context_impl(ContextType::Volatile);
        let mut var = set.get_or_new("foo".into(), Scope::Volatile);
        var.assign(Value::scalar("VOLATILE"), Some(Location::dummy("anywhere")))
            .unwrap();
        var.export(true);

        let mut var = set.get_or_new("foo".into(), Scope::Global);

        assert_eq!(var.value, Some(Value::scalar("VOLATILE")));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("anywhere")),
        );
        var.assign(Value::scalar("NEW"), Some(Location::dummy("somewhere")))
            .unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        set.pop_context_impl();
        // The value DUMMY is now gone.
        // The value VOLATILE has been overwritten by NEW.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some(Value::scalar("NEW")));
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
        let mut var = set.get_or_new("foo".into(), Scope::Local);
        var.assign(Value::scalar("ONE"), None).unwrap();
        set.push_context_impl(ContextType::Regular);
        let mut var = set.get_or_new("foo".into(), Scope::Local);
        var.assign(Value::scalar("TWO"), None).unwrap();
        set.push_context_impl(ContextType::Regular);
        set.push_context_impl(ContextType::Volatile);
        let mut var = set.get_or_new("foo".into(), Scope::Volatile);
        var.assign(Value::scalar("VOLATILE"), Some(Location::dummy("anywhere")))
            .unwrap();
        var.export(true);

        let mut var = set.get_or_new("foo".into(), Scope::Global);

        assert_eq!(var.value, Some(Value::scalar("VOLATILE")));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("anywhere")),
        );
        var.assign(Value::scalar("NEW"), Some(Location::dummy("somewhere")))
            .unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        // The value TWO has been overwritten by NEW.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some(Value::scalar("NEW")));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("somewhere")),
        );
        // But it's still exported.
        assert!(var.is_exported);
        set.pop_context_impl();
        // The value ONE is still there.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some(Value::scalar("ONE")));
    }

    #[test]
    fn lowering_volatile_variable_to_topmost_regular_context_without_existing_variable() {
        let mut set = VariableSet::new();
        set.push_context_impl(ContextType::Regular);
        set.push_context_impl(ContextType::Regular);
        set.push_context_impl(ContextType::Volatile);
        let mut var = set.get_or_new("foo".into(), Scope::Volatile);
        var.assign(Value::scalar("DUMMY"), None).unwrap();
        set.push_context_impl(ContextType::Volatile);
        let mut var = set.get_or_new("foo".into(), Scope::Volatile);
        var.assign(Value::scalar("VOLATILE"), Some(Location::dummy("anywhere")))
            .unwrap();
        var.export(true);

        let mut var = set.get_or_new("foo".into(), Scope::Local);

        assert_eq!(var.value, Some(Value::scalar("VOLATILE")));
        assert_eq!(
            var.last_assigned_location,
            Some(Location::dummy("anywhere")),
        );
        var.assign(Value::scalar("NEW"), Some(Location::dummy("somewhere")))
            .unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        // The value DUMMY is now gone.
        // The value VOLATILE has been overwritten by NEW.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some(Value::scalar("NEW")));
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
        set.push_context_impl(ContextType::Regular);
        set.push_context_impl(ContextType::Regular);
        let mut var = set.get_or_new("foo".into(), Scope::Local);
        var.assign(Value::scalar("OLD"), None).unwrap();
        set.push_context_impl(ContextType::Volatile);
        let mut var = set.get_or_new("foo".into(), Scope::Volatile);
        var.assign(Value::scalar("DUMMY"), None).unwrap();
        set.push_context_impl(ContextType::Volatile);
        let mut var = set.get_or_new("foo".into(), Scope::Volatile);
        var.assign(Value::scalar("VOLATILE"), Some(Location::dummy("first")))
            .unwrap();
        var.export(true);
        set.push_context_impl(ContextType::Volatile);

        let mut var = set.get_or_new("foo".into(), Scope::Local);

        assert_eq!(var.value, Some(Value::scalar("VOLATILE")));
        assert_eq!(var.last_assigned_location, Some(Location::dummy("first")));
        var.assign(Value::scalar("NEW"), Some(Location::dummy("second")))
            .unwrap();
        set.assert_normalized();
        set.pop_context_impl();
        set.pop_context_impl();
        set.pop_context_impl();
        // The value DUMMY is now gone.
        // The value OLD has been overwritten by NEW.
        let var = set.get("foo").unwrap();
        assert_eq!(var.value, Some(Value::scalar("NEW")));
        assert_eq!(var.last_assigned_location, Some(Location::dummy("second")));
        // But it's still exported.
        assert!(var.is_exported);
    }

    #[test]
    #[should_panic(expected = "no volatile context to store the variable")]
    fn missing_volatile_context() {
        let mut set = VariableSet::new();
        set.get_or_new("foo".into(), Scope::Volatile);
    }

    #[test]
    fn unsetting_nonexisting_variable() {
        let mut variables = VariableSet::new();
        let result = variables.unset(Scope::Global, "").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn unsetting_variable_with_one_context() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new("X"))
            .unwrap();

        let result = variables.unset(Scope::Global, "foo").unwrap();
        assert_eq!(result, Some(Variable::new("X")));
        assert_eq!(variables.get("foo"), None);
    }

    #[test]
    fn unsetting_variables_from_all_contexts() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new("X"))
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new("Y"))
            .unwrap();
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("Z"))
            .unwrap();

        let result = variables.unset(Scope::Global, "foo").unwrap();
        assert_eq!(result, Some(Variable::new("Z")));
        assert_eq!(variables.get("foo"), None);
    }

    #[test]
    fn unsetting_variable_from_local_context() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new("A"))
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        // Non-local read-only variable does not prevent unsetting
        let value = Variable::new("B").make_read_only(Location::dummy("dummy"));
        variables
            .assign(Scope::Local, "foo".to_string(), value.clone())
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new("C"))
            .unwrap();
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("D"))
            .unwrap();

        let result = variables.unset(Scope::Local, "foo").unwrap();
        assert_eq!(result, Some(Variable::new("D")));
        assert_eq!(variables.get("foo"), Some(&value));
    }

    #[test]
    fn unsetting_nonexisting_variable_in_local_context() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new("A"))
            .unwrap();
        variables.push_context_impl(ContextType::Regular);

        let result = variables.unset(Scope::Local, "foo").unwrap();
        assert_eq!(result, None);
        assert_eq!(variables.get("foo"), Some(&Variable::new("A")));
    }

    #[test]
    fn unsetting_variable_from_volatile_context() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new("A"))
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new("B"))
            .unwrap();
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("C"))
            .unwrap();
        variables.push_context_impl(ContextType::Volatile);
        variables
            .assign(Scope::Volatile, "foo".to_string(), Variable::new("D"))
            .unwrap();

        let result = variables.unset(Scope::Volatile, "foo").unwrap();
        assert_eq!(result, Some(Variable::new("D")));
        assert_eq!(variables.get("foo"), Some(&Variable::new("B")));
    }

    #[test]
    fn unsetting_nonexisting_variable_in_volatile_context() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new("A"))
            .unwrap();
        variables.push_context_impl(ContextType::Volatile);

        let result = variables.unset(Scope::Volatile, "foo").unwrap();
        assert_eq!(result, None);
        assert_eq!(variables.get("foo"), Some(&Variable::new("A")));
    }

    #[test]
    fn unsetting_readonly_variable() {
        let read_only_location = &Location::dummy("read-only");
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Global, "foo".to_string(), Variable::new("A"))
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(
                Scope::Local,
                "foo".to_string(),
                Variable::new("B").make_read_only(Location::dummy("dummy")),
            )
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(
                Scope::Local,
                "foo".to_string(),
                Variable::new("C").make_read_only(read_only_location.clone()),
            )
            .unwrap();
        variables.push_context_impl(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), Variable::new("D"))
            .unwrap();

        let error = variables.unset(Scope::Global, "foo").unwrap_err();
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

    #[test]
    fn exporting() {
        let mut variables = VariableSet::new();
        let variable = Variable::new("first");
        variables
            .assign(Scope::Local, "foo".to_string(), variable)
            .unwrap();
        let variable = Variable::new("second").export();
        let old_value = variables
            .assign(Scope::Local, "foo".to_string(), variable)
            .unwrap()
            .unwrap();
        assert_eq!(old_value.value, Some(Value::scalar("first")));
        assert!(!old_value.is_exported);
        let new_value = variables.get("foo").unwrap();
        assert_eq!(new_value.value, Some(Value::scalar("second")));
        assert!(new_value.is_exported);
    }

    #[test]
    fn reexport_on_reassigning_exported_variable() {
        let mut variables = VariableSet::new();
        let variable = Variable::new("first").export();
        variables
            .assign(Scope::Local, "foo".to_string(), variable)
            .unwrap();
        let old_value = variables
            .assign(Scope::Local, "foo".to_string(), Variable::new("second"))
            .unwrap()
            .unwrap();
        assert_eq!(old_value.value, Some(Value::scalar("first")));
        assert!(old_value.is_exported);
        let new_value = variables.get("foo").unwrap();
        assert_eq!(new_value.value, Some(Value::scalar("second")));
        assert!(new_value.is_exported);
    }

    fn test_iter<F: FnOnce(&VariableSet)>(f: F) {
        let mut set = VariableSet::new();

        set.assign(
            Scope::Global,
            "global".to_string(),
            Variable::new("global value").export(),
        )
        .unwrap();
        set.assign(
            Scope::Global,
            "local".to_string(),
            Variable::new("hidden value").export(),
        )
        .unwrap();

        let mut set = set.push_context(ContextType::Regular);

        set.assign(
            Scope::Local,
            "local".to_string(),
            Variable::new("visible value"),
        )
        .unwrap();
        set.assign(
            Scope::Local,
            "volatile".to_string(),
            Variable::new("hidden value"),
        )
        .unwrap();

        let mut set = set.push_context(ContextType::Volatile);

        set.assign(
            Scope::Volatile,
            "volatile".to_string(),
            Variable::new("volatile value"),
        )
        .unwrap();

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

        variables
            .assign(
                Scope::Global,
                "foo".to_string(),
                Variable::new("FOO").export(),
            )
            .unwrap();
        variables
            .assign(
                Scope::Global,
                "bar".to_string(),
                Variable::new_array(["BAR"]).export(),
            )
            .unwrap();
        variables
            .assign(
                Scope::Global,
                "baz".to_string(),
                Variable::new_array(["1", "two", "3"]).export(),
            )
            .unwrap();
        variables
            .assign(
                Scope::Global,
                "null".to_string(),
                Variable::new("not exported"),
            )
            .unwrap();
        variables
            .assign(Scope::Global, "none".to_string(), Variable::default())
            .unwrap();
        let mut ss = variables.env_c_strings();
        ss.sort_unstable();
        assert_eq!(
            &ss,
            &[
                CString::new("bar=BAR").unwrap(),
                CString::new("baz=1:two:3").unwrap(),
                CString::new("foo=FOO").unwrap()
            ]
        );
    }

    #[test]
    fn extend_env() {
        let mut variables = VariableSet::new();

        variables.extend_env([("foo", "FOO"), ("bar", "OK")]);

        let foo = variables.get("foo").unwrap();
        assert_eq!(foo.value, Some(Value::scalar("FOO")));
        assert!(foo.is_exported);
        let bar = variables.get("bar").unwrap();
        assert_eq!(bar.value, Some(Value::scalar("OK")));
        assert!(bar.is_exported);
    }

    #[test]
    fn init_lineno() {
        let mut variables = VariableSet::new();
        variables.init();
        let v = variables.get("LINENO").unwrap();
        assert_eq!(v.value, None);
        assert_eq!(v.quirk, Some(Quirk::LineNumber));
        assert_eq!(v.last_assigned_location, None);
        assert!(!v.is_exported);
        assert_eq!(v.read_only_location, None);
    }

    #[test]
    fn positional_params_in_base_context() {
        let mut variables = VariableSet::new();
        assert_eq!(variables.positional_params().value, Some(Array(vec![])));

        let v = variables.positional_params_mut();
        assert_matches!(&mut v.value, Some(Array(values)) => {
            values.push("foo".to_string());
            values.push("bar".to_string());
        });

        assert_matches!(&variables.positional_params().value, Some(Array(values)) => {
            assert_eq!(values.as_ref(), ["foo".to_string(), "bar".to_string()]);
        });
    }

    #[test]
    fn positional_params_in_second_regular_context() {
        let mut variables = VariableSet::new();
        variables.push_context_impl(ContextType::Regular);
        assert_eq!(variables.positional_params().value, Some(Array(vec![])));

        let v = variables.positional_params_mut();
        assert_matches!(&mut v.value, Some(Array(values)) => {
            values.push("1".to_string());
        });

        assert_matches!(&variables.positional_params().value, Some(Array(values)) => {
            assert_eq!(values.as_ref(), ["1".to_string()]);
        });
    }

    #[test]
    fn getting_positional_params_in_volatile_context() {
        let mut variables = VariableSet::new();

        let v = variables.positional_params_mut();
        assert_matches!(&mut v.value, Some(Array(values)) => {
            values.push("a".to_string());
            values.push("b".to_string());
            values.push("c".to_string());
        });

        variables.push_context_impl(ContextType::Volatile);
        assert_matches!(&variables.positional_params().value, Some(Array(values)) => {
            assert_eq!(values.as_ref(), ["a".to_string(), "b".to_string(), "c".to_string()]);
        });
    }

    #[test]
    fn setting_positional_params_in_volatile_context() {
        let mut variables = VariableSet::new();
        variables.push_context_impl(ContextType::Volatile);

        let v = variables.positional_params_mut();
        assert_matches!(&mut v.value, Some(Array(values)) => {
            values.push("x".to_string());
        });

        variables.pop_context_impl();
        assert_matches!(&variables.positional_params().value, Some(Array(values)) => {
            assert_eq!(values.as_ref(), ["x".to_string()]);
        });
    }
}
