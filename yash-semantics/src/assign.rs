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
use crate::expansion::AssignReadOnlyError;
use crate::xtrace::XTrace;
use std::fmt::Write;
use yash_env::semantics::ExitStatus;
use yash_env::Env;

#[doc(no_inline)]
pub use crate::expansion::{Error, ErrorCause, Result};
#[doc(no_inline)]
pub use yash_env::variable::Scope;
#[doc(no_inline)]
pub use yash_syntax::syntax::Assign;

/// Performs an assignment.
///
/// This function [expands the value](expand_value) and then
/// [assigns](yash_env::variable::VariableRefMut::assign) it to the environment.
/// The return value is the exit status of the last command substitution
/// performed during the expansion of the assigned value, if any
///
/// If `xtrace` is `Some` instance of `XTrace`, the expanded assignment word is
/// written to its main buffer.
pub async fn perform_assignment(
    env: &mut Env,
    assign: &Assign,
    scope: Scope,
    export: bool,
    xtrace: Option<&mut XTrace>,
) -> Result<Option<ExitStatus>> {
    let name = assign.name.clone();
    let (value, exit_status) = expand_value(env, &assign.value).await?;

    if let Some(xtrace) = xtrace {
        write!(
            xtrace.main(),
            "{}={} ",
            yash_quote::quoted(&name),
            value.quote()
        )
        .unwrap();
    }

    let mut variable = env.get_or_create_variable(name, scope);
    variable
        .assign(value, assign.location.clone())
        .map_err(|e| Error {
            cause: ErrorCause::AssignReadOnly(AssignReadOnlyError {
                name: assign.name.clone(),
                new_value: e.new_value,
                read_only_location: e.read_only_location,
            }),
            location: e.assigned_location.unwrap(),
        })?;
    if export {
        variable.export(true);
    }
    Ok(exit_status)
}

/// Performs assignments.
///
/// This function calls [`perform_assignment`] for each [`Assign`].
/// The return value is the exit status of the last command substitution
/// performed during the expansion of the assigned values, if any
///
/// If `xtrace` is `Some` instance of `XTrace`, the expanded assignment words
/// are written to its main buffer.
pub async fn perform_assignments(
    env: &mut Env,
    assigns: &[Assign],
    scope: Scope,
    export: bool,
    mut xtrace: Option<&mut XTrace>,
) -> Result<Option<ExitStatus>> {
    let mut exit_status = None;
    for assign in assigns {
        let new_exit_status =
            perform_assignment(env, assign, scope, export, xtrace.as_deref_mut()).await?;
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
    use yash_env::variable::Variable;
    use yash_syntax::source::Location;

    #[test]
    fn perform_assignment_new_value() {
        let mut env = Env::new_virtual();
        let a: Assign = "foo=bar".parse().unwrap();
        let exit_status = perform_assignment(&mut env, &a, Scope::Global, false, None)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(exit_status, None);
        assert_eq!(
            env.variables.get("foo").unwrap(),
            &Variable::new("bar").set_assigned_location(a.location)
        );
    }

    #[test]
    fn perform_assignment_overwriting() {
        let mut env = Env::new_virtual();
        let a: Assign = "foo=bar".parse().unwrap();
        perform_assignment(&mut env, &a, Scope::Global, false, None)
            .now_or_never()
            .unwrap()
            .unwrap();

        let a: Assign = "foo=baz".parse().unwrap();
        let exit_status = perform_assignment(&mut env, &a, Scope::Global, true, None)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(exit_status, None);
        assert_eq!(
            env.variables.get("foo").unwrap(),
            &Variable::new("baz")
                .export()
                .set_assigned_location(a.location)
        );

        let a: Assign = "foo=foo".parse().unwrap();
        let exit_status = perform_assignment(&mut env, &a, Scope::Global, false, None)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(exit_status, None);
        // The variable is still exported.
        assert_eq!(
            env.variables.get("foo").unwrap(),
            &Variable::new("foo")
                .export()
                .set_assigned_location(a.location)
        );
    }

    #[test]
    fn perform_assignment_read_only() {
        let mut env = Env::new_virtual();
        let location = Location::dummy("read-only location");
        let mut var = env.variables.get_or_new("v", Scope::Global);
        var.assign("read-only", None).unwrap();
        var.make_read_only(location.clone());
        let a: Assign = "v=new".parse().unwrap();
        let e = perform_assignment(&mut env, &a, Scope::Global, false, None)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_matches!(e.cause, ErrorCause::AssignReadOnly(error) => {
            assert_eq!(error.name, "v");
            assert_eq!(error.new_value, Value::scalar("new"));
            assert_eq!(error.read_only_location, location);
        });
        assert_eq!(e.location, Location::dummy("v=new"));
    }

    #[test]
    fn perform_assignment_with_xtrace() {
        let mut xtrace = XTrace::new();
        let mut env = Env::new_virtual();

        let a: Assign = "foo=bar${unset-&}".parse().unwrap();
        let _ = perform_assignment(&mut env, &a, Scope::Global, false, Some(&mut xtrace))
            .now_or_never()
            .unwrap()
            .unwrap();

        let a: Assign = "one=1".parse().unwrap();
        let _ = perform_assignment(&mut env, &a, Scope::Global, false, Some(&mut xtrace))
            .now_or_never()
            .unwrap()
            .unwrap();

        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "foo='bar&' one=1\n");
    }

    #[test]
    fn perform_assignments_exit_status() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("return", return_builtin());
            let assigns = [
                "a=A$(return -n 1)".parse().unwrap(),
                "b=($(return -n 2))".parse().unwrap(),
            ];
            let exit_status = perform_assignments(&mut env, &assigns, Scope::Global, false, None)
                .await
                .unwrap();
            assert_eq!(exit_status, Some(ExitStatus(2)));
            assert_eq!(
                env.variables.get("a").unwrap(),
                &Variable::new("A").set_assigned_location(assigns[0].location.clone())
            );
            assert_eq!(
                env.variables.get("b").unwrap(),
                &Variable::new_empty_array().set_assigned_location(assigns[1].location.clone())
            );
        })
    }
}
