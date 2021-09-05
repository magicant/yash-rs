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

//! Assignment.

use crate::expansion::expand_value;
use yash_env::variable::ReadOnlyError;
use yash_env::variable::Variable;
use yash_env::Env;
use yash_syntax::source::Location;

#[doc(no_inline)]
pub use yash_syntax::syntax::Assign;

/// Types of errors that may occur in assignments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorCause {
    /// Assignment to a read-only variable.
    ReadOnly {
        /// Variable name.
        name: String,
        /// Location where the existing variable was made read-only.
        read_only_location: Location,
    },
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

// TODO Export or not?
// TODO Specifying the scope of assignment
/// Performs an assignment.
///
/// This function [expands the value](expand_value) and then
/// [assigns](yash_env::variable::VariableSet::assign) it to the environment.
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
        Err(ReadOnlyError {
            name,
            read_only_location,
            new_value,
        }) => {
            let cause = ErrorCause::ReadOnly {
                name,
                read_only_location,
            };
            let location = new_value.last_assigned_location.unwrap();
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
        let location = Location::dummy("read-only location");
        let v = Variable {
            value: Value::Scalar("read-only".to_string()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: Some(location.clone()),
        };
        env.variables.assign("v".to_string(), v).unwrap();
        let a: Assign = "v=new".parse().unwrap();
        let e = block_on(perform_assignment(&mut env, &a)).unwrap_err();
        assert_matches!(e.cause, ErrorCause::ReadOnly{name, read_only_location} => {
            assert_eq!(name, "v");
            assert_eq!(read_only_location, location);
        });
        assert_eq!(e.location.line.value, "v=new");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.column.get(), 1);
    }
}
