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
use yash_env::variable::Variable;

#[doc(no_inline)]
pub use crate::expansion::{Env, Error, ErrorCause, Result};
#[doc(no_inline)]
pub use yash_env::variable::Scope;
#[doc(no_inline)]
pub use yash_syntax::syntax::Assign;

/// Performs an assignment.
///
/// This function [expands the value](expand_value) and then
/// [assigns](yash_env::variable::VariableSet::assign) it to the environment.
pub async fn perform_assignment<E: Env>(
    env: &mut E,
    assign: &Assign,
    scope: Scope,
    export: bool,
) -> Result {
    let name = assign.name.clone();
    let value = expand_value(env, &assign.value).await?;
    let value = Variable {
        value,
        last_assigned_location: Some(assign.location.clone()),
        is_exported: export,
        read_only_location: None,
    };
    env.assign_variable(scope, name, value).map_err(|e| Error {
        cause: ErrorCause::AssignReadOnly(e),
        location: assign.location.clone(),
    })?;
    Ok(())
}

/// Performs assignments.
///
/// This function calls [`perform_assignment`] for each [`Assign`].
pub async fn perform_assignments<E: Env>(
    env: &mut E,
    assigns: &[Assign],
    scope: Scope,
    export: bool,
) -> Result {
    for assign in assigns {
        perform_assignment(env, assign, scope, export).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use futures_executor::block_on;
    use yash_env::variable::Value;
    use yash_env::Env;
    use yash_syntax::source::Location;

    #[test]
    fn perform_assignment_new_value() {
        let mut env = Env::new_virtual();
        let a: Assign = "foo=bar".parse().unwrap();
        block_on(perform_assignment(&mut env, &a, Scope::Global, false)).unwrap();
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
    fn perform_assignment_overwriting() {
        let mut env = Env::new_virtual();
        let a: Assign = "foo=bar".parse().unwrap();
        block_on(perform_assignment(&mut env, &a, Scope::Global, false)).unwrap();
        let a: Assign = "foo=baz".parse().unwrap();
        block_on(perform_assignment(&mut env, &a, Scope::Global, true)).unwrap();
        assert_eq!(
            env.variables.get("foo").unwrap(),
            &Variable {
                value: Value::Scalar("baz".to_string()),
                last_assigned_location: Some(a.location),
                is_exported: true,
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
        env.variables
            .assign(Scope::Global, "v".to_string(), v)
            .unwrap();
        let a: Assign = "v=new".parse().unwrap();
        let e = block_on(perform_assignment(&mut env, &a, Scope::Global, false)).unwrap_err();
        assert_matches!(e.cause, ErrorCause::AssignReadOnly(roe) => {
            assert_eq!(roe.name, "v");
            assert_eq!(roe.read_only_location, location);
            assert_eq!(roe.new_value.value, Value::Scalar("new".into()));
        });
        assert_eq!(e.location.line.value, "v=new");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.column.get(), 1);
    }
}
