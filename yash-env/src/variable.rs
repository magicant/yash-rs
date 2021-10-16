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

//! Type definitions for variables.
//!
//! This module provides data types for defining shell variables.

use crate::Env;
use either::{Left, Right};
use itertools::Itertools;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::ffi::CString;
use std::fmt::Write;
use std::hash::Hash;
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
    /// Splits the value by colons.
    ///
    /// If this value is `Scalar`, the value is separated at each occurrence of
    /// colon (`:`). For `Array`, each array item is returned without further
    /// splitting the value.
    ///
    /// ```
    /// # use yash_env::variable::Value::Scalar;
    /// let scalar = Scalar("/usr/local/bin:/usr/bin:/bin".to_string());
    /// let values: Vec<&str> = scalar.split().collect();
    /// assert_eq!(values, ["/usr/local/bin", "/usr/bin", "/bin"]);
    /// ```
    ///
    /// ```
    /// # use yash_env::variable::Value::Array;
    /// let array = Array(vec!["foo".to_string(), "bar".to_string()]);
    /// let values: Vec<&str> = array.split().collect();
    /// assert_eq!(values, ["foo", "bar"]);
    /// ```
    pub fn split(&self) -> impl Iterator<Item = &str> {
        match self {
            Scalar(value) => Left(value.split(':')),
            Array(values) => Right(values.iter().map(String::as_str)),
        }
    }
}

/// Definition of a variable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Variable {
    /// Value of the variable.
    pub value: Value,

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
    /// Whether this variable is read-only or not.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only_location.is_some()
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

/// Collection of variables.
///
/// A `VariableSet` is a stack of contexts, and a _context_ is a map of
/// name-variable pairs. The `VariableSet` has a _base context_ that has the
/// same lifetime as the `VariableSet` itself. Additional contexts can be pushed
/// and popped on a last-in-first-out basis.
///
/// You can define any number of variables in a context. A new context is empty
/// when pushed. You can pop a context regardless of whether it is empty or not;
/// all the variables in the context are removed together.
///
/// Variables in a context hide those with the same name in lower contexts. You
/// cannot access such hidden variables until you remove the hiding variable
/// from the upper context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariableSet {
    /// Hash map containing all variables.
    ///
    /// The value of a hash map entry is a stack of variables defined in
    /// contexts, sorted in the ascending order of the context index.
    all_variables: HashMap<String, Vec<VariableInContext>>,

    /// Stack of contexts.
    contexts: Vec<ContextType>,
}

impl Default for VariableSet {
    fn default() -> Self {
        VariableSet {
            all_variables: Default::default(),
            contexts: vec![ContextType::Regular],
        }
    }
}

/// Choice of a context to which a variable is assigned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    /// Assigns to as lower a context as possible.
    ///
    /// The variable is assigned to the base context if there are no other
    /// variables having the same name in any context. Otherwise, the assignment
    /// will overwrite the topmost existing variable.
    Global,

    /// Assigns to the topmost context.
    ///
    /// Any existing variables in the contexts below the topmost do not affect
    /// this type of assignment.
    ///
    /// If the `VariableSet` only has the base context, the variable is assigned
    /// to it anyway.
    Local,
    // TODO Volatile
}

/// Error that occurs when assigning to an existing read-only variable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadOnlyError {
    /// Variable name.
    pub name: String,
    /// Location where the existing variable was made read-only.
    pub read_only_location: Location,
    /// New variable that was tried to assign.
    pub new_value: Variable,
}

impl VariableSet {
    /// Creates an empty variable set.
    #[must_use]
    pub fn new() -> VariableSet {
        Default::default()
    }

    /// Gets a reference to the variable with the specified name.
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
    /// existing read-only value, the assignment fails.
    pub fn assign(
        &mut self,
        scope: Scope,
        name: String,
        mut value: Variable,
    ) -> Result<Option<Variable>, ReadOnlyError> {
        use std::collections::hash_map::Entry;
        // TODO Can we avoid cloning the name here?
        let stack = match self.all_variables.entry(name.clone()) {
            Entry::Vacant(vacant) => vacant.insert(Vec::new()),
            Entry::Occupied(occupied) => occupied.into_mut(),
        };
        let context_count = self.contexts.len();
        let existing = stack
            .last_mut()
            .map(|vic| match scope {
                Scope::Global => Some(&mut vic.variable),
                Scope::Local => {
                    if vic.context_index == context_count - 1 {
                        Some(&mut vic.variable)
                    } else {
                        None
                    }
                }
            })
            .flatten();

        if let Some(existing) = existing {
            if let Some(location) = &existing.read_only_location {
                return Err(ReadOnlyError {
                    name,
                    read_only_location: location.clone(),
                    new_value: value,
                });
            }

            value.is_exported |= existing.is_exported;
            Ok(Some(std::mem::replace(existing, value)))
        } else {
            let context_index = match scope {
                Scope::Global => 0,
                Scope::Local => self.contexts.len() - 1,
            };
            let variable = VariableInContext {
                variable: value,
                context_index,
            };
            stack.push(variable);
            Ok(None)
        }
    }

    /// Pushes a new empty context.
    pub fn push_context(&mut self, context_type: ContextType) {
        self.contexts.push(context_type);
    }

    /// Pops the last-pushed context.
    ///
    /// This function removes the topmost context from the internal stack of
    /// contexts in the `VariableSet`, thereby removing all the variables in the
    /// context.
    ///
    /// You cannot pop the base context. This function would __panic__ if you
    /// tried to do so.
    pub fn pop_context(&mut self) {
        debug_assert!(!self.contexts.is_empty());
        if self.contexts.len() == 1 {
            panic!("cannot pop the base context");
        }
        self.contexts.pop();
        // TODO Use HashMap::drain_filter to remove empty values
        // TODO Use complementary stack of hash tables to avoid scanning the
        // whole `self.all_variables`
        for stack in self.all_variables.values_mut() {
            if let Some(vic) = stack.last() {
                if vic.context_index >= self.contexts.len() {
                    stack.pop();
                }
            }
        }
    }

    /// Returns environment variables in a new vector of C string.
    #[must_use]
    pub fn env_c_strings(&self) -> Vec<CString> {
        self.all_variables
            .iter()
            .filter_map(|(name, vars)| {
                let var = &vars.last()?.variable;
                if var.is_exported {
                    let mut s = name.clone();
                    s.push('=');
                    match &var.value {
                        Scalar(value) => s.push_str(value),
                        Array(values) => write!(s, "{}", values.iter().format(":")).ok()?,
                    }
                    // TODO return something rather than dropping null-containing strings
                    CString::new(s).ok()
                } else {
                    None
                }
            })
            .collect()
    }
}

/// RAII-style guard for temporarily retaining a variable context.
///
/// The [`Env::push_variable_context`] function returns a `ScopeGuard` that
/// keeps a reference to the environment. When you drop the `ScopeGuard`, it
/// pops the context.
#[derive(Debug)]
#[must_use = "You must retain ScopeGuard to keep the context alive"]
pub struct ScopeGuard<'a> {
    env: &'a mut Env,
}

impl<'a> ScopeGuard<'a> {
    pub(crate) fn new(env: &'a mut Env, context_type: ContextType) -> Self {
        env.variables.push_context(context_type);
        ScopeGuard { env }
    }
}

impl std::ops::Drop for ScopeGuard<'_> {
    /// Drops the `ScopeGuard`.
    ///
    /// This function [pops](VariableSet::pop_context) the context that was
    /// pushed when creating this `ScopeGuard`.
    fn drop(&mut self) {
        self.env.variables.pop_context();
    }
}

impl std::ops::Deref for ScopeGuard<'_> {
    type Target = Env;
    fn deref(&self) -> &Env {
        self.env
    }
}

impl std::ops::DerefMut for ScopeGuard<'_> {
    fn deref_mut(&mut self) -> &mut Env {
        self.env
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assign_new_variable_and_get() {
        let mut variables = VariableSet::new();
        let variable = Variable {
            value: Scalar("my value".to_string()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: Some(Location::dummy("dummy")),
        };
        let result = variables
            .assign(Scope::Global, "foo".to_string(), variable.clone())
            .unwrap();
        assert_eq!(result, None);
        assert_eq!(variables.get("foo"), Some(&variable));
    }

    #[test]
    fn reassign_variable_and_get() {
        let mut variables = VariableSet::new();
        let v1 = Variable {
            value: Scalar("my value".to_string()),
            last_assigned_location: Some(Location::dummy("dummy")),
            is_exported: false,
            read_only_location: None,
        };
        variables
            .assign(Scope::Global, "foo".to_string(), v1.clone())
            .unwrap();

        let v2 = Variable {
            value: Scalar("your value".to_string()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: Some(Location::dummy("something")),
        };
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
        let v1 = Variable {
            value: Scalar("my value".to_string()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: Some(read_only_location.clone()),
        };
        variables
            .assign(Scope::Global, "x".to_string(), v1.clone())
            .unwrap();

        let v2 = Variable {
            value: Scalar("your value".to_string()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: Some(Location::dummy("something")),
        };
        let error = variables
            .assign(Scope::Global, "x".to_string(), v2.clone())
            .unwrap_err();
        assert_eq!(error.name, "x");
        assert_eq!(error.read_only_location, read_only_location);
        assert_eq!(error.new_value, v2);
        assert_eq!(variables.get("x"), Some(&v1));
    }

    fn dummy_variable<V: Into<String>>(value: V) -> Variable {
        Variable {
            value: Scalar(value.into()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: None,
        }
    }

    #[test]
    fn assign_global() {
        let mut variables = VariableSet::new();
        variables.push_context(ContextType::Regular);
        variables
            .assign(Scope::Global, "foo".to_string(), dummy_variable(""))
            .unwrap();
        variables.pop_context();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Scalar("".to_string()));
    }

    #[test]
    fn assign_local() {
        let mut variables = VariableSet::new();
        variables.push_context(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), dummy_variable(""))
            .unwrap();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Scalar("".to_string()));
    }

    #[test]
    fn popping_context_removes_variables() {
        let mut variables = VariableSet::new();
        variables.push_context(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), dummy_variable(""))
            .unwrap();
        variables.pop_context();
        assert_eq!(variables.get("foo"), None);
    }

    #[test]
    fn reassign_global_non_base_context() {
        let mut variables = VariableSet::new();
        variables.push_context(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), dummy_variable("a"))
            .unwrap();
        variables.push_context(ContextType::Regular);
        variables
            .assign(Scope::Global, "foo".to_string(), dummy_variable("b"))
            .unwrap();
        variables.pop_context();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Scalar("b".to_string()));
        variables.pop_context();
        assert_eq!(variables.get("foo"), None);
    }

    #[test]
    fn variable_in_upper_context_hides_lower_variables() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Local, "foo".to_string(), dummy_variable("0"))
            .unwrap();
        variables.push_context(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), dummy_variable("1"))
            .unwrap();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Scalar("1".to_string()));
    }

    #[test]
    fn variable_is_visible_again_after_popping_upper_variables() {
        let mut variables = VariableSet::new();
        variables
            .assign(Scope::Local, "foo".to_string(), dummy_variable("0"))
            .unwrap();
        variables.push_context(ContextType::Regular);
        variables
            .assign(Scope::Local, "foo".to_string(), dummy_variable("1"))
            .unwrap();
        variables.pop_context();
        let variable = variables.get("foo").unwrap();
        assert_eq!(variable.value, Scalar("0".to_string()));
    }

    #[test]
    #[should_panic(expected = "cannot pop the base context")]
    fn cannot_pop_base_context() {
        let mut variables = VariableSet::new();
        variables.pop_context();
    }

    #[test]
    fn exporting() {
        let mut variables = VariableSet::new();
        let variable = Variable {
            value: Scalar("first".to_string()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: None,
        };
        variables
            .assign(Scope::Local, "foo".to_string(), variable)
            .unwrap();
        let variable = Variable {
            value: Scalar("second".to_string()),
            last_assigned_location: None,
            is_exported: true,
            read_only_location: None,
        };
        let old_value = variables
            .assign(Scope::Local, "foo".to_string(), variable)
            .unwrap()
            .unwrap();
        assert_eq!(old_value.value, Scalar("first".to_string()));
        assert!(!old_value.is_exported);
        let new_value = variables.get("foo").unwrap();
        assert_eq!(new_value.value, Scalar("second".to_string()));
        assert!(new_value.is_exported);
    }

    #[test]
    fn reexport_on_reassigning_exported_variable() {
        let mut variables = VariableSet::new();
        let variable = Variable {
            value: Scalar("first".to_string()),
            last_assigned_location: None,
            is_exported: true,
            read_only_location: None,
        };
        variables
            .assign(Scope::Local, "foo".to_string(), variable)
            .unwrap();
        let variable = Variable {
            value: Scalar("second".to_string()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: None,
        };
        let old_value = variables
            .assign(Scope::Local, "foo".to_string(), variable)
            .unwrap()
            .unwrap();
        assert_eq!(old_value.value, Scalar("first".to_string()));
        assert!(old_value.is_exported);
        let new_value = variables.get("foo").unwrap();
        assert_eq!(new_value.value, Scalar("second".to_string()));
        assert!(new_value.is_exported);
    }

    #[test]
    fn env_c_strings() {
        let mut variables = VariableSet::new();
        assert_eq!(&variables.env_c_strings(), &[]);

        variables
            .assign(
                Scope::Global,
                "foo".to_string(),
                Variable {
                    value: Scalar("FOO".to_string()),
                    last_assigned_location: None,
                    is_exported: true,
                    read_only_location: None,
                },
            )
            .unwrap();
        variables
            .assign(
                Scope::Global,
                "bar".to_string(),
                Variable {
                    value: Array(vec!["BAR".to_string()]),
                    last_assigned_location: None,
                    is_exported: true,
                    read_only_location: None,
                },
            )
            .unwrap();
        variables
            .assign(
                Scope::Global,
                "baz".to_string(),
                Variable {
                    value: Array(vec!["1".to_string(), "two".to_string(), "3".to_string()]),
                    last_assigned_location: None,
                    is_exported: true,
                    read_only_location: None,
                },
            )
            .unwrap();
        variables
            .assign(
                Scope::Global,
                "null".to_string(),
                Variable {
                    value: Scalar("not exported".to_string()),
                    last_assigned_location: None,
                    is_exported: false,
                    read_only_location: None,
                },
            )
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
    fn scope_guard() {
        let mut env = Env::new_virtual();
        let mut guard = env.push_variable_context(ContextType::Regular);
        guard
            .variables
            .assign(Scope::Global, "foo".to_string(), dummy_variable(""))
            .unwrap();
        guard
            .variables
            .assign(Scope::Local, "bar".to_string(), dummy_variable(""))
            .unwrap();
        Env::pop_variable_context(guard);

        let variable = env.variables.get("foo").unwrap();
        assert_eq!(variable.value, Scalar("".to_string()));
        assert_eq!(env.variables.get("bar"), None);
    }
}
