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
use either::{Left, Right};
use itertools::Itertools;
use std::borrow::Borrow;
use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::CString;
use std::fmt::Write;
use std::hash::Hash;
use std::iter::FusedIterator;
use thiserror::Error;
use yash_syntax::source::Location;

/// Value of a variable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    /// Single string.
    Scalar(String),
    /// Array of strings.
    Array(Vec<String>),
}

pub use Value::*;

impl Value {
    /// Creates a scalar value.
    #[must_use]
    pub fn scalar<S: Into<String>>(value: S) -> Self {
        Scalar(value.into())
    }

    /// Creates an array value.
    #[must_use]
    pub fn array<I, S>(values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Array(values.into_iter().map(Into::into).collect())
    }

    /// Splits the value by colons.
    ///
    /// If this value is `Scalar`, the value is separated at each occurrence of
    /// colon (`:`). For `Array`, each array item is returned without further
    /// splitting the value.
    ///
    /// ```
    /// # use yash_env::variable::Value;
    /// let scalar = Value::scalar("/usr/local/bin:/usr/bin:/bin");
    /// let values: Vec<&str> = scalar.split().collect();
    /// assert_eq!(values, ["/usr/local/bin", "/usr/bin", "/bin"]);
    /// ```
    ///
    /// ```
    /// # use yash_env::variable::Value;
    /// let array = Value::array(vec!["foo", "bar"]);
    /// let values: Vec<&str> = array.split().collect();
    /// assert_eq!(values, ["foo", "bar"]);
    /// ```
    pub fn split(&self) -> impl Iterator<Item = &str> {
        match self {
            Scalar(value) => Left(value.split(':')),
            Array(values) => Right(values.iter().map(String::as_str)),
        }
    }

    /// Quotes the value in a format suitable for re-parsing.
    ///
    /// This function returns a temporary wrapper of `self`. To obtain a string
    /// representation of the quoted value, you can use the `Display` or
    /// `Into<Cow<str>>` implementation for the returned object.
    ///
    /// See [`yash_quote`] for details of quoting.
    ///
    /// ```
    /// # use yash_env::variable::Value;
    /// let scalar = Value::scalar("foo bar");
    /// assert_eq!(scalar.quote().to_string(), "'foo bar'");
    /// let array = Value::array(vec!["1", "", "'\\'"]);
    /// assert_eq!(array.quote().to_string(), r#"(1 '' "'\\'")"#);
    /// ```
    pub fn quote(&self) -> QuotedValue {
        QuotedValue::from(self)
    }
}

/// Wrapper of [`Value`] for [quoting](Value::quote).
#[derive(Clone, Copy, Debug)]
pub struct QuotedValue<'a> {
    value: &'a Value,
}

/// Writes a quoted version of the value to the formatter.
impl<'a> std::fmt::Display for QuotedValue<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.value {
            Scalar(value) => yash_quote::quoted(value).fmt(f),
            Array(values) => write!(
                f,
                "({})",
                values
                    .iter()
                    .format_with(" ", |value, f| f(&yash_quote::quoted(value)))
            ),
        }
    }
}

/// Wraps a value in `QuotedValue`.
impl<'a> From<&'a Value> for QuotedValue<'a> {
    fn from(value: &'a Value) -> Self {
        QuotedValue { value }
    }
}

/// Extracts the wrapped reference to the value.
impl AsRef<Value> for QuotedValue<'_> {
    fn as_ref(&self) -> &Value {
        self.value
    }
}

/// Constructs a quoted string.
impl<'a> From<QuotedValue<'a>> for Cow<'a, str> {
    fn from(value: QuotedValue<'a>) -> Self {
        match value.value {
            Scalar(value) => yash_quote::quote(value),
            Array(_values) => value.to_string().into(),
        }
    }
}

mod quirk;

pub use self::quirk::Expansion;
pub use self::quirk::Quirk;

/// Definition of a variable.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Variable {
    /// Value of the variable.
    ///
    /// The value is `None` if the variable has been declared without
    /// assignment.
    pub value: Option<Value>,

    /// Special characteristics of the variable
    ///
    /// See [`Quirk`] and [`expand`](Self::expand) for details.
    pub quirk: Option<Quirk>,

    /// Optional location where this variable was assigned.
    ///
    /// If the current variable value originates from an assignment performed in
    /// the shell session, `last_assigned_location` is the location of the
    /// assignment.  Otherwise, `last_assigned_location` is `None`.
    pub last_assigned_location: Option<Location>,

    /// Whether this variable is exported or not.
    ///
    /// An exported variable is also referred to as an _environment variable_.
    pub is_exported: bool,

    /// Optional location where this variable was made read-only.
    ///
    /// If this variable is not read-only, `read_only_location` is `None`.
    /// Otherwise, `read_only_location` is the location of the simple command
    /// that executed the `readonly` built-in that made this variable read-only.
    pub read_only_location: Option<Location>,
}

impl Variable {
    /// Creates a new scalar variable from a string.
    ///
    /// The returned variable's `last_assigned_location` and
    /// `read_only_location` are `None` and `is_exported` is false.
    /// You should update these fields as necessary before assigning to a
    /// variable set.
    #[must_use]
    pub fn new<S: Into<String>>(value: S) -> Self {
        Variable {
            value: Some(Value::scalar(value)),
            ..Default::default()
        }
    }

    /// Creates a new array variable from a string.
    ///
    /// The returned variable's `last_assigned_location` and
    /// `read_only_location` are `None` and `is_exported` is false.
    /// You should update these fields as necessary before assigning to a
    /// variable set.
    #[must_use]
    pub fn new_array<I, S>(values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Variable {
            value: Some(Value::array(values)),
            ..Default::default()
        }
    }

    /// Creates a new empty array variable.
    ///
    /// The returned variable's `last_assigned_location` and
    /// `read_only_location` are `None` and `is_exported` is false.
    /// You should update these fields as necessary before assigning to a
    /// variable set.
    #[must_use]
    pub fn new_empty_array() -> Self {
        Self::new_array([] as [&str; 0])
    }

    /// Sets the last assigned location.
    ///
    /// This is a convenience function for doing
    /// `self.last_assigned_location = Some(location)` in a method chain.
    #[inline]
    #[must_use]
    pub fn set_assigned_location(mut self, location: Location) -> Self {
        self.last_assigned_location = Some(location);
        self
    }

    /// Sets the `is_exported` flag.
    ///
    /// This is a convenience function for doing `self.is_exported = true` in a
    /// method chain.
    #[inline]
    #[must_use]
    pub fn export(mut self) -> Self {
        self.is_exported = true;
        self
    }

    /// Makes the variable read-only.
    ///
    /// This is a convenience function for doing
    /// `self.read_only_location = Some(location)` in a method chain.
    #[inline]
    #[must_use]
    pub fn make_read_only(mut self, location: Location) -> Self {
        self.read_only_location = Some(location);
        self
    }

    /// Whether this variable is read-only or not.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only_location.is_some()
    }

    // TODO Should require mutable self
    /// Returns the value of this variable, applying any quirk.
    ///
    /// If this variable has no [`Quirk`], this function just returns
    /// `self.value` converted to [`Expansion`]. Otherwise, the effect of the
    /// quirk is applied to the value and the result is returned.
    ///
    /// This function requires the location of the parameter expanding this
    /// variable, so that `Quirk::LineNumber` can yield the line number of the
    /// location.
    pub fn expand(&self, location: &Location) -> Expansion {
        self::quirk::expand(self, location)
    }
}

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

    /// Computes the index of the context that matches the specified scope.
    fn index_of_context(scope: Scope, contexts: &[Context]) -> usize {
        fn index_of_topmost_regular_context(contexts: &[Context]) -> usize {
            contexts
                .iter()
                .rposition(|context| context.r#type == ContextType::Regular)
                .expect("base context has gone")
        }

        match scope {
            Scope::Global => 0,
            Scope::Local => index_of_topmost_regular_context(contexts),
            Scope::Volatile => index_of_topmost_regular_context(contexts) + 1,
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
        &self
            .contexts
            .iter()
            .filter(|c| c.r#type == ContextType::Regular)
            .next_back()
            .expect("base context has gone")
            .positional_params
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
        &mut self
            .contexts
            .iter_mut()
            .filter(|c| c.r#type == ContextType::Regular)
            .next_back()
            .expect("base context has gone")
            .positional_params
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
