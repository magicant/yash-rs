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
use yash_syntax::source::Location;

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
        super::quirk::expand(self, location)
    }
}
