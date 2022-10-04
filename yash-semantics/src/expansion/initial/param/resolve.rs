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

//! Resolving parameter names to values

use std::borrow::Cow;
use yash_env::variable::Value;

/// Result of parameter name resolution
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Resolve<'a> {
    Unset,
    Scalar(Cow<'a, str>),
    Array(Cow<'a, [String]>),
}

impl From<String> for Resolve<'static> {
    fn from(value: String) -> Self {
        Resolve::Scalar(Cow::Owned(value))
    }
}

impl<'a> From<&'a str> for Resolve<'a> {
    fn from(value: &'a str) -> Self {
        Resolve::Scalar(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for Resolve<'a> {
    fn from(value: &'a String) -> Self {
        Resolve::Scalar(Cow::Borrowed(value))
    }
}

impl From<Vec<String>> for Resolve<'static> {
    fn from(values: Vec<String>) -> Self {
        Resolve::Array(Cow::Owned(values))
    }
}

impl<'a> From<&'a [String]> for Resolve<'a> {
    fn from(values: &'a [String]) -> Self {
        Resolve::Array(Cow::Borrowed(values))
    }
}

impl<'a> From<&'a Vec<String>> for Resolve<'a> {
    fn from(values: &'a Vec<String>) -> Self {
        Resolve::Array(Cow::Borrowed(values))
    }
}

impl From<Value> for Resolve<'static> {
    fn from(value: Value) -> Self {
        match value {
            Value::Scalar(value) => Resolve::from(value),
            Value::Array(values) => Resolve::from(values),
        }
    }
}

impl<'a> From<&'a Value> for Resolve<'a> {
    fn from(value: &'a Value) -> Self {
        match value {
            Value::Scalar(value) => Resolve::from(value),
            Value::Array(values) => Resolve::from(values),
        }
    }
}

impl Resolve<'_> {
    /// Converts into an owned value
    pub fn into_owned(self) -> Option<Value> {
        match self {
            Resolve::Unset => None,
            Resolve::Scalar(value) => Some(Value::Scalar(value.into_owned())),
            Resolve::Array(values) => Some(Value::Array(values.into_owned())),
        }
    }

    /// Returns the "length" of the value.
    ///
    /// For `Unset`, the length is 0.
    /// For `Scalar`, the length is the number of characters.
    /// For `Array`, the length is the number of strings.
    pub fn len(&self) -> usize {
        match self {
            Resolve::Unset => 0,
            Resolve::Scalar(value) => value.len(),
            Resolve::Array(values) => values.len(),
        }
    }
}
