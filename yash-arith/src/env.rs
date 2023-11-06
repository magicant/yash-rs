// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Variable environment

use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::ops::Range;

/// Interface for accessing variables during evaluation
///
/// This crate does not implement any mechanism for storing variables. The
/// caller of [`eval`](crate::eval()) must provide an implementation of this
/// trait, which is used to access variables that appear in the evaluated
/// expression.
pub trait Env {
    /// Object returned on a variable access error
    type GetVariableError;

    /// Object returned on an assignment error
    type AssignVariableError;

    /// Returns the value of the specified variable.
    ///
    /// This function must return:
    ///
    /// - `Ok(Some(v))` if the variable is defined and has the value `v`,
    /// - `Ok(None)` if the variable is not defined, or
    /// - `Err(error)` if an error occurs.
    fn get_variable(&self, name: &str) -> Result<Option<&str>, Self::GetVariableError>;

    /// Assigns a new value to the specified variable.
    ///
    /// The `location` parameter is the index range to the evaluated expression
    /// where the assignment appears.
    fn assign_variable(
        &mut self,
        name: &str,
        value: String,
        location: Range<usize>,
    ) -> Result<(), Self::AssignVariableError>;
}

impl Env for HashMap<String, String> {
    type GetVariableError = Infallible;
    type AssignVariableError = Infallible;

    fn get_variable(&self, name: &str) -> Result<Option<&str>, Infallible> {
        Ok(self.get(name).map(String::as_str))
    }

    fn assign_variable(
        &mut self,
        name: &str,
        value: String,
        _location: Range<usize>,
    ) -> Result<(), Infallible> {
        self.insert(name.to_owned(), value);
        Ok(())
    }
}

impl Env for BTreeMap<String, String> {
    type GetVariableError = Infallible;
    type AssignVariableError = Infallible;

    fn get_variable(&self, name: &str) -> Result<Option<&str>, Infallible> {
        Ok(self.get(name).map(String::as_str))
    }

    fn assign_variable(
        &mut self,
        name: &str,
        value: String,
        _location: Range<usize>,
    ) -> Result<(), Infallible> {
        self.insert(name.to_owned(), value);
        Ok(())
    }
}
