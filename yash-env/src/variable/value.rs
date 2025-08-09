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

use either::{Left, Right};
use itertools::Itertools;
use std::borrow::Cow;

/// Value of a variable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    /// Single string.
    Scalar(String),
    /// Array of strings.
    Array(Vec<String>),
}

use Value::*;

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
    pub fn quote(&self) -> QuotedValue<'_> {
        QuotedValue::from(self)
    }
}

/// Converts a string into a scalar value.
impl From<String> for Value {
    fn from(value: String) -> Self {
        Scalar(value)
    }
}

/// Converts a string slice to a scalar value.
impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Scalar(value.to_owned())
    }
}

/// Converts a vector of strings into an array value.
impl From<Vec<String>> for Value {
    fn from(values: Vec<String>) -> Self {
        Array(values)
    }
}

/// Wrapper of [`Value`] for [quoting](Value::quote).
#[derive(Clone, Copy, Debug)]
pub struct QuotedValue<'a> {
    value: &'a Value,
}

/// Writes a quoted version of the value to the formatter.
impl std::fmt::Display for QuotedValue<'_> {
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
