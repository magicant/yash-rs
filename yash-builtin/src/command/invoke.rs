// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Command invoking semantics

use super::Invoke;
use super::identify::NotFound;
use super::search::SearchEnv;
use crate::common::report::report_failure;
use yash_env::Env;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_semantics::command::simple_command::execute_function_body;
use yash_semantics::command::simple_command::start_external_utility_in_subshell_and_wait;
use yash_semantics::command_search::Target;
use yash_semantics::command_search::search;

impl Invoke {
    /// Execute the command
    pub async fn execute(self, env: &mut Env) -> crate::Result {
        let Some(name) = self.fields.first() else {
            return crate::Result::default();
        };

        let params = &self.search;
        let search_env = &mut SearchEnv { env, params };
        let Some(target) = search(search_env, &name.value) else {
            let mut result = report_failure(env, &NotFound { name }).await;
            result.set_exit_status(ExitStatus::NOT_FOUND);
            return result;
        };

        invoke_target(env, target, self.fields).await
    }
}

/// Invokes the target with the given fields.
///
/// This function is called after the command is found. The first field must be
/// the command name that was searched for. The rest of the fields are the
/// arguments to the command.
async fn invoke_target(env: &mut Env, target: Target, mut fields: Vec<Field>) -> crate::Result {
    match target {
        Target::Builtin { builtin, .. } => {
            let frame = yash_env::stack::Builtin {
                name: fields.remove(0),
                // Any built-in is considered non-special in the command built-in.
                is_special: false,
            };
            let mut env = env.push_frame(frame.into());
            (builtin.execute)(&mut env, fields).await
        }

        Target::Function(function) => {
            let divert = execute_function_body(env, function, fields, |_| ()).await;
            crate::Result::with_exit_status_and_divert(env.exit_status, divert)
        }

        Target::External { path } => {
            use std::ops::ControlFlow::{Break, Continue};
            match start_external_utility_in_subshell_and_wait(env, path, fields).await {
                Continue(exit_status) => exit_status.into(),
                Break(divert) => {
                    crate::Result::with_exit_status_and_divert(env.exit_status, Break(divert))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Search;
    use super::*;
    use assert_matches::assert_matches;
    use enumset::EnumSet;
    use futures_util::FutureExt as _;
    use std::ffi::CString;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::builtin::Builtin;
    use yash_env::builtin::Type::Special;
    use yash_env::function::Function;
    use yash_env::semantics::Field;
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;
    use yash_semantics::Divert::Return;
    use yash_syntax::source::Location;
    use yash_syntax::syntax::FullCompoundCommand;

    #[test]
    fn empty_command_invocation() {
        let mut env = Env::new_virtual();
        let invoke = Invoke::default();
        let result = invoke.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, crate::Result::default());
    }

    #[test]
    fn command_not_found() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.builtins
            .insert("foo", Builtin::new(Special, |_, _| unreachable!()));
        let invoke = Invoke {
            fields: Field::dummies(["foo"]),
            search: Search {
                standard_path: false,
                categories: EnumSet::empty(),
            },
        };

        let result = invoke.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result.exit_status(), ExitStatus::NOT_FOUND);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("not found"), "stderr: {stderr:?}");
        });
    }

    #[test]
    fn invoking_builtin() {
        fn make_result() -> yash_env::builtin::Result {
            let mut result = crate::Result::default();
            result.set_exit_status(ExitStatus(79));
            result.set_divert(Break(Return(None)));
            result.retain_redirs();
            result
        }

        let mut env = Env::new_virtual();
        let target = Target::Builtin {
            builtin: Builtin::new(Special, |_, args| {
                Box::pin(async move {
                    assert_eq!(args, Field::dummies(["bar", "baz"]));
                    make_result()
                })
            }),
            path: CString::default(),
        };

        let result = invoke_target(&mut env, target, Field::dummies(["foo", "bar", "baz"]))
            .now_or_never()
            .unwrap();
        assert_eq!(result, make_result());
    }

    #[test]
    fn invoking_function() {
        let mut env = Env::new_virtual();
        env.builtins.insert(
            ":",
            Builtin::new(Special, |_, args| {
                Box::pin(async move {
                    assert_matches!(args.as_slice(), [bar, baz] => {
                        assert_eq!(bar.value, "bar");
                        assert_eq!(baz.value, "baz");
                    });
                    crate::Result::with_exit_status_and_divert(ExitStatus(42), Break(Return(None)))
                })
            }),
        );
        let body: FullCompoundCommand = "{ : \"$@\"; }".parse().unwrap();
        let origin = Location::dummy("some location");
        let target = Target::Function(Rc::new(Function::new("foo", body, origin)));

        let result = invoke_target(&mut env, target, Field::dummies(["foo", "bar", "baz"]))
            .now_or_never()
            .unwrap();
        assert_eq!(result, crate::Result::from(ExitStatus(42)));
    }
}
