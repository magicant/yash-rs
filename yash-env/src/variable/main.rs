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

//! Module that defines the main `Variable` type.

use super::Expansion;
use super::Quirk;
use super::Value;
use std::ops::Deref;
use thiserror::Error;
use yash_syntax::source::Location;

/// Definition of a variable.
///
/// The methods of `Variable` are designed to be used in a method chain,
/// but you usually don't create a `Variable` instance directly.
/// Instead, use [`VariableSet::get_or_new`](super::VariableSet::get_or_new) or
/// [`Env::get_or_create_variable`](crate::Env::get_or_create_variable) to
/// create a variable in a variable set and obtain a mutable reference to it
/// ([`VariableRefMut`]), which allows you to modify the variable.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Variable {
    /// Value of the variable.
    ///
    /// The value is `None` if the variable has been declared without
    /// assignment.
    pub value: Option<Value>,

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

    /// Special characteristics of the variable
    ///
    /// See [`Quirk`] and [`expand`](Self::expand) for details.
    pub quirk: Option<Quirk>,
}

impl Variable {
    /// Creates a new scalar variable from a string.
    ///
    /// The returned variable's `last_assigned_location` and
    /// `read_only_location` are `None` and `is_exported` is false.
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
        super::quirk::expand(self, location)
    }
}

/// Managed mutable reference to a variable.
///
/// This type allows you to mutate a variable in a variable set while
/// maintaining the invariants of the variable set. To obtain an instance of
/// `VariableRefMut`, use (TODO TBD).
#[derive(Debug, Eq, PartialEq)]
pub struct VariableRefMut<'a>(&'a mut Variable);

/// Error that occurs when assigning a value to a read-only variable.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cannot assign to read-only variable")]
pub struct AssignError {
    /// Value that was being assigned.
    pub new_value: Value,
    /// Location of the failed assignment.
    pub assigned_location: Option<Location>,
    /// Location where the variable was made read-only.
    pub read_only_location: Location,
}

impl<'a> From<&'a mut Variable> for VariableRefMut<'a> {
    fn from(variable: &'a mut Variable) -> Self {
        VariableRefMut(variable)
    }
}

impl Deref for VariableRefMut<'_> {
    type Target = Variable;

    fn deref(&self) -> &Variable {
        self.0
    }
}

impl<'a> VariableRefMut<'a> {
    /// Assigns a value to this variable.
    ///
    /// The `value` and `location` operands are set to the `value` and
    /// `last_assigned_location` fields of this variable, respectively.
    /// If successful, this function returns the previous value and location.
    ///
    /// This function fails if this variable is read-only. In that case, the
    /// error contains the given operands as well as the location where this
    /// variable was made read-only.
    #[inline]
    pub fn assign<V: Into<Value>, L: Into<Option<Location>>>(
        &mut self,
        value: V,
        location: L,
    ) -> Result<(Option<Value>, Option<Location>), AssignError> {
        self.assign_impl(value.into(), location.into())
    }

    fn assign_impl(
        &mut self,
        value: Value,
        location: Option<Location>,
    ) -> Result<(Option<Value>, Option<Location>), AssignError> {
        if let Some(read_only_location) = self.0.read_only_location.clone() {
            return Err(AssignError {
                new_value: value,
                assigned_location: location,
                read_only_location,
            });
        }

        let old_value = std::mem::replace(&mut self.0.value, Some(value));
        let old_location = std::mem::replace(&mut self.0.last_assigned_location, location);
        Ok((old_value, old_location))
        // TODO Apply quirk
    }

    /// Sets whether this variable is exported or not.
    pub fn export(&mut self, is_exported: bool) {
        self.0.is_exported = is_exported;
    }

    /// Makes this variable read-only.
    ///
    /// The `location` operand is set to the `read_only_location` field of this
    /// variable unless this variable is already read-only.
    pub fn make_read_only(&mut self, location: Location) {
        self.0.read_only_location.get_or_insert(location);
    }

    /// Sets the quirk of this variable.
    ///
    /// This function overwrites any existing quirk of this variable.
    pub fn set_quirk(&mut self, quirk: Option<Quirk>) {
        self.0.quirk = quirk;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigning_values() {
        let mut var = Variable::default();
        let mut var = VariableRefMut::from(&mut var);
        let result = var.assign(Value::scalar("foo value"), None);
        assert_eq!(result, Ok((None, None)));
        assert_eq!(*var, Variable::new("foo value"));

        let location = Location::dummy("bar location");
        let result = var.assign(Value::scalar("bar value"), location.clone());
        assert_eq!(result, Ok((Some(Value::scalar("foo value")), None)));
        assert_eq!(var.value, Some(Value::scalar("bar value")));
        assert_eq!(var.last_assigned_location.as_ref(), Some(&location));

        assert_eq!(
            var.assign(Value::array(["a", "b", "c"]), None),
            Ok((Some(Value::scalar("bar value")), Some(location))),
        );
        assert_eq!(var.value, Some(Value::array(["a", "b", "c"])));
    }

    #[test]
    fn exporting() {
        let mut var = Variable::default();
        let mut var = VariableRefMut::from(&mut var);
        assert!(!var.is_exported);
        var.export(true);
        assert!(var.is_exported);
        var.export(false);
        assert!(!var.is_exported);
    }

    #[test]
    fn making_variables_read_only() {
        let mut var = Variable::default();
        let mut var = VariableRefMut::from(&mut var);
        let location = Location::dummy("read-only location");
        var.make_read_only(location.clone());
        assert_eq!(var.read_only_location.as_ref(), Some(&location));

        var.make_read_only(Location::dummy("ignored location"));
        assert_eq!(var.read_only_location.as_ref(), Some(&location));
    }

    #[test]
    fn assigning_to_readonly_variable() {
        let mut var = Variable::default();
        let mut var = VariableRefMut::from(&mut var);
        let assigned_location = Some(Location::dummy("assigned location"));
        let read_only_location = Location::dummy("read-only location");
        var.make_read_only(read_only_location.clone());
        assert_eq!(
            var.assign(Value::scalar("foo value"), assigned_location.clone()),
            Err(AssignError {
                new_value: Value::scalar("foo value"),
                assigned_location,
                read_only_location,
            })
        )
    }
}
