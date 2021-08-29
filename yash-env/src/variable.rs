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
            Value::Scalar(value) => Left(value.split(':')),
            Value::Array(values) => Right(values.iter().map(String::as_str)),
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

/// Collection of variables.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VariableSet(HashMap<String, Variable>);
// TODO Support local scopes

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
        self.0.get(name)
    }

    // TODO Reject if the existing variable is read-only
    // TODO Export if the existing variable has been exported
    // TODO Specifying a scope to assign in
    /// Assigns a variable.
    ///
    /// Returns the old value, if any.
    pub fn assign(&mut self, name: String, value: Variable) -> Option<Variable> {
        self.0.insert(name, value)
    }

    /// Returns environment variables in a new vector of C string.
    #[must_use]
    pub fn env_c_strings(&self) -> Vec<CString> {
        self.0
            .iter()
            .filter_map(|(name, var)| {
                if var.is_exported {
                    let mut s = name.clone();
                    s.push('=');
                    match &var.value {
                        Value::Scalar(value) => s.push_str(value),
                        Value::Array(values) => write!(s, "{}", values.iter().format(":")).ok()?,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assign_new_variable_and_get() {
        let mut variables = VariableSet::new();
        let variable = Variable {
            value: Value::Scalar("my value".to_string()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: Some(Location::dummy("dummy")),
        };
        variables.assign("foo".to_string(), variable.clone());
        assert_eq!(variables.get("foo"), Some(&variable));
    }

    #[test]
    fn env_c_strings() {
        let mut variables = VariableSet::new();
        assert_eq!(&variables.env_c_strings(), &[]);

        variables.assign(
            "foo".to_string(),
            Variable {
                value: Value::Scalar("FOO".to_string()),
                last_assigned_location: None,
                is_exported: true,
                read_only_location: None,
            },
        );
        variables.assign(
            "bar".to_string(),
            Variable {
                value: Value::Array(vec!["BAR".to_string()]),
                last_assigned_location: None,
                is_exported: true,
                read_only_location: None,
            },
        );
        variables.assign(
            "baz".to_string(),
            Variable {
                value: Value::Array(vec!["1".to_string(), "two".to_string(), "3".to_string()]),
                last_assigned_location: None,
                is_exported: true,
                read_only_location: None,
            },
        );
        variables.assign(
            "null".to_string(),
            Variable {
                value: Value::Scalar("not exported".to_string()),
                last_assigned_location: None,
                is_exported: false,
                read_only_location: None,
            },
        );
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
}
