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

//! Assignment semantics.

use crate::expansion::expand_word;
use crate::expansion::expand_words;
use yash_env::variable::Value;
use yash_env::variable::Variable;
use yash_env::Env;
use yash_syntax::source::Location;

#[doc(no_inline)]
pub use yash_syntax::syntax::Assign;

/// Types of errors that may occur in assignments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorCause {
    /// Assignment to a read-only variable.
    ReadOnly { name: String },
    /// Expansion error.
    Expansion(crate::expansion::ErrorCause),
}

impl From<crate::expansion::ErrorCause> for ErrorCause {
    fn from(cause: crate::expansion::ErrorCause) -> Self {
        ErrorCause::Expansion(cause)
    }
}

/// Explanation of an assignment error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub cause: ErrorCause,
    pub location: Location,
}

impl From<crate::expansion::Error> for Error {
    fn from(error: crate::expansion::Error) -> Self {
        Error {
            cause: error.cause.into(),
            location: error.location,
        }
    }
}

/// Result of assignment.
pub type Result<T = ()> = std::result::Result<T, Error>;

/// Expands the value.
pub async fn expand_value(env: &mut Env, value: &yash_syntax::syntax::Value) -> Result<Value> {
    match value {
        yash_syntax::syntax::Scalar(word) => {
            let field = expand_word(env, word).await.map_err(Error::from)?;
            Ok(Value::Scalar(field.value))
        }
        yash_syntax::syntax::Array(words) => {
            let fields = expand_words(env, words).await.map_err(Error::from)?;
            let fields = fields.into_iter().map(|f| f.value).collect();
            Ok(Value::Array(fields))
        }
    }
}

// TODO Export or not?
// TODO Specifying the scope of assignment
/// Performs an assignment.
pub async fn perform_assignment(env: &mut Env, assign: &Assign) -> Result {
    let name = assign.name.clone();
    let value = expand_value(env, &assign.value).await?;
    let value = Variable {
        value,
        last_assigned_location: Some(assign.location.clone()),
        is_exported: false,
        read_only_location: None,
    };
    match env.variables.assign(name, value) {
        Ok(_old_value) => Ok(()),
        Err(value) => {
            let name = assign.name.clone();
            let cause = ErrorCause::ReadOnly { name };
            let location = value.last_assigned_location.unwrap();
            Err(Error { cause, location })
        }
    }
}

/// Performs assignments.
///
/// This function calls [`perform_assignment`] for each [`Assign`].
pub async fn perform_assignments(env: &mut Env, assigns: &[Assign]) -> Result {
    for assign in assigns {
        perform_assignment(env, assign).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use futures_executor::block_on;
    use yash_env::variable::Value;

    #[test]
    fn expand_value_scalar() {
        let mut env = Env::new_virtual();
        let v = yash_syntax::syntax::Scalar(r"1\\".parse().unwrap());
        let result = block_on(expand_value(&mut env, &v)).unwrap();
        let content = assert_matches!(result, Value::Scalar(content) => content);
        assert_eq!(content, r"1\");
    }

    #[test]
    fn expand_value_array() {
        let mut env = Env::new_virtual();
        let v = yash_syntax::syntax::Array(vec!["''".parse().unwrap(), r"2\\".parse().unwrap()]);
        let result = block_on(expand_value(&mut env, &v)).unwrap();
        let content = assert_matches!(result, Value::Array(content) => content);
        assert_eq!(content, ["".to_string(), r"2\".to_string()]);
    }

    #[test]
    fn perform_assignment_new_value() {
        let mut env = Env::new_virtual();
        let a: Assign = "foo=bar".parse().unwrap();
        block_on(perform_assignment(&mut env, &a)).unwrap();
        assert_eq!(
            env.variables.get("foo").unwrap(),
            &Variable {
                value: Value::Scalar("bar".to_string()),
                last_assigned_location: Some(a.location),
                is_exported: false,
                read_only_location: None,
            }
        );
    }

    #[test]
    fn perform_assignment_read_only() {
        let mut env = Env::new_virtual();
        let v = Variable {
            value: Value::Scalar("read-only".to_string()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: Some(Location::dummy("")),
        };
        env.variables.assign("v".to_string(), v).unwrap();
        let a: Assign = "v=new".parse().unwrap();
        let e = block_on(perform_assignment(&mut env, &a)).unwrap_err();
        let name = assert_matches!(e.cause, ErrorCause::ReadOnly{name} => name);
        assert_eq!(name, "v");
        assert_eq!(e.location.line.value, "v=new");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.column.get(), 1);
    }
}
