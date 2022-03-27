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
use yash_env::semantics::ExitStatus;
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
pub async fn perform_assignment(
    env: &mut yash_env::Env,
    assign: &Assign,
    scope: Scope,
    export: bool,
) -> Result<Option<ExitStatus>> {
    let name = assign.name.clone();
    let (value, exit_status) = expand_value(env, &assign.value).await?;
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
    Ok(exit_status)
}

/// Performs assignments.
///
/// This function calls [`perform_assignment`] for each [`Assign`].
pub async fn perform_assignments(
    env: &mut yash_env::Env,
    assigns: &[Assign],
    scope: Scope,
    export: bool,
) -> Result<Option<ExitStatus>> {
    let mut exit_status = None;
    for assign in assigns {
        let new_exit_status = perform_assignment(env, assign, scope, export).await?;
        exit_status = new_exit_status.or(exit_status);
    }
    Ok(exit_status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use yash_env::variable::Value;
    use yash_env::Env;
    use yash_syntax::source::Location;

    #[test]
    fn perform_assignment_new_value() {
        let mut env = Env::new_virtual();
        let a: Assign = "foo=bar".parse().unwrap();
        let exit_status = perform_assignment(&mut env, &a, Scope::Global, false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(exit_status, None);
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
        let exit_status = perform_assignment(&mut env, &a, Scope::Global, false)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(exit_status, None);
        let a: Assign = "foo=baz".parse().unwrap();
        let exit_status = perform_assignment(&mut env, &a, Scope::Global, true)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(exit_status, None);
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
        let e = perform_assignment(&mut env, &a, Scope::Global, false)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_matches!(e.cause, ErrorCause::AssignReadOnly(roe) => {
            assert_eq!(roe.name, "v");
            assert_eq!(roe.read_only_location, location);
            assert_eq!(roe.new_value.value, Value::Scalar("new".into()));
        });
        assert_eq!(*e.location.code.value.borrow(), "v=new");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.range, 0..5);
    }

    #[test]
    fn perform_assignments_exit_status() {
        in_virtual_system(|mut env, _pid, _state| async move {
            env.builtins.insert("return", return_builtin());
            let assigns = [
                "a=A$(return -n 1)".parse().unwrap(),
                "b=($(return -n 2))".parse().unwrap(),
            ];
            let exit_status = perform_assignments(&mut env, &assigns, Scope::Global, false)
                .await
                .unwrap();
            assert_eq!(exit_status, Some(ExitStatus(2)));
            assert_eq!(
                env.variables.get("a").unwrap(),
                &Variable {
                    value: Value::Scalar("A".to_string()),
                    last_assigned_location: Some(assigns[0].location.clone()),
                    is_exported: false,
                    read_only_location: None,
                }
            );
            assert_eq!(
                env.variables.get("b").unwrap(),
                &Variable {
                    value: Value::Array(vec!["".to_string()]),
                    last_assigned_location: Some(assigns[1].location.clone()),
                    is_exported: false,
                    read_only_location: None,
                }
            );
        })
    }
}
