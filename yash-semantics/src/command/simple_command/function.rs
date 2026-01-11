// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Simple command semantics for functions

use super::perform_assignments;
use crate::Handle;
use crate::Runtime;
use crate::redir::RedirGuard;
use crate::xtrace::XTrace;
use crate::xtrace::print;
use crate::xtrace::trace_fields;
use std::ops::ControlFlow::{Break, Continue};
use std::pin::Pin;
use std::rc::Rc;
use yash_env::Env;
use yash_env::function::Function;
use yash_env::semantics::Divert;
use yash_env::semantics::Field;
use yash_env::semantics::Result;
use yash_env::variable::Context;
use yash_env::variable::PositionalParams;
use yash_syntax::syntax::Assign;
use yash_syntax::syntax::Redir;

pub async fn execute_function<S: Runtime + 'static>(
    env: &mut Env<S>,
    function: Rc<Function<S>>,
    assigns: &[Assign],
    fields: Vec<Field>,
    redirs: &[Redir],
) -> Result {
    let env = &mut RedirGuard::new(env);
    let mut xtrace = XTrace::from_options(&env.options);
    if let Err(e) = env.perform_redirs(redirs, xtrace.as_mut()).await {
        return e.handle(env).await;
    };

    let mut env = env.push_context(Context::Volatile);
    perform_assignments(&mut env, assigns, true, xtrace.as_mut()).await?;

    trace_fields(xtrace.as_mut(), &fields);
    print(&mut env, xtrace).await;

    execute_function_body(&mut env, function, fields, None).await
}

type EnvPrepHook<S> = fn(&mut Env<S>) -> Pin<Box<dyn Future<Output = ()> + '_>>;

/// Executes the body of the function.
///
/// The given function is executed in the given environment. The fields are
/// passed as positional parameters to the function except for the first field
/// which is the name of the function.
///
/// `env_prep_hook` is called after the new variable context is pushed to the
/// environment. This is useful for assigning custom local variables before the
/// function body is executed.
pub async fn execute_function_body<S: Runtime>(
    env: &mut Env<S>,
    function: Rc<Function<S>>,
    fields: Vec<Field>,
    env_prep_hook: Option<EnvPrepHook<S>>,
) -> Result {
    let positional_params = PositionalParams::from_fields(fields);
    let mut env = env.push_context(Context::Regular { positional_params });
    if let Some(hook) = env_prep_hook {
        hook(&mut env).await;
    }

    // TODO Update control flow stack
    let result = function.body.execute(&mut env).await;
    if let Break(Divert::Return(exit_status)) = result {
        if let Some(exit_status) = exit_status {
            env.exit_status = exit_status;
        }
        Continue(())
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command as _;
    use crate::command::function_definition::BodyImpl;
    use crate::tests::echo_builtin;
    use crate::tests::local_builtin;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::VirtualSystem;
    use yash_env::function::FunctionBodyObject;
    use yash_env::option::State::On;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::variable::Scope;
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;
    use yash_syntax::source::Location;
    use yash_syntax::syntax::SimpleCommand;

    fn function_body_impl(src: &str) -> Rc<dyn FunctionBodyObject<VirtualSystem>> {
        Rc::new(BodyImpl(src.parse().unwrap()))
    }

    #[test]
    fn simple_command_returns_exit_status_from_function() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let function = Function::new(
            "foo",
            function_body_impl("{ return -n 13; }"),
            Location::dummy("dummy"),
        );
        env.functions.define(function).unwrap();
        let command: SimpleCommand = "foo".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(13));
    }

    #[test]
    fn simple_command_applies_redirections_to_function() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.builtins.insert("echo", echo_builtin());
        let function = Function::new(
            "foo",
            function_body_impl("{ echo ok; }"),
            Location::dummy("dummy"),
        );
        env.functions.define(function).unwrap();
        let command: SimpleCommand = "foo >/tmp/file".parse().unwrap();

        _ = command.execute(&mut env).now_or_never().unwrap();
        let file = state.borrow().file_system.get("/tmp/file").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content), Ok("ok\n"));
        });
    }

    #[test]
    fn simple_command_skips_running_function_on_redirection_error() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.builtins.insert("echo", echo_builtin());
        let function = Function::new(
            "foo",
            function_body_impl("{ echo ok; }"),
            Location::dummy("dummy"),
        );
        env.functions.define(function).unwrap();
        let command: SimpleCommand = "a=v foo </no/such/file".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::ERROR);
        assert_eq!(env.variables.get("a"), None);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn function_call_consumes_return_without_exit_status() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let function = Function::new(
            "foo",
            function_body_impl("{ return; }"),
            Location::dummy("dummy"),
        );
        env.functions.define(function).unwrap();
        env.exit_status = ExitStatus(17);
        let command: SimpleCommand = "foo".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(17));
    }

    #[test]
    fn function_call_consumes_return_with_exit_status() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let function = Function::new(
            "foo",
            function_body_impl("{ return 26; }"),
            Location::dummy("dummy"),
        );
        env.functions.define(function).unwrap();
        let command: SimpleCommand = "foo".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(26));
    }

    #[test]
    fn simple_command_passes_arguments_to_function() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.builtins.insert("echo", echo_builtin());
        let function = Function::new(
            "foo",
            function_body_impl("{ echo $1-$2-$3; }"),
            Location::dummy("dummy"),
        );
        env.functions.define(function).unwrap();
        let command: SimpleCommand = "foo bar baz".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "bar-baz-\n"));
    }

    #[test]
    fn simple_command_creates_temporary_context_executing_function() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("local", local_builtin());
        let function = Function::new(
            "foo",
            function_body_impl("{ local x=42; echo $x; }"),
            Location::dummy("dummy"),
        );
        env.functions.define(function).unwrap();
        let command: SimpleCommand = "foo".parse().unwrap();

        _ = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(env.variables.get("x"), None);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "42\n"));
    }

    #[test]
    fn simple_command_performs_function_assignment_in_temporary_context() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.builtins.insert("echo", echo_builtin());
        let function = Function::new(
            "foo",
            function_body_impl("{ echo $x; }"),
            Location::dummy("dummy"),
        );
        env.functions.define(function).unwrap();
        let command: SimpleCommand = "x=hello foo".parse().unwrap();

        _ = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(env.variables.get("x"), None);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "hello\n"));
    }

    #[test]
    fn function_fails_on_reassigning_to_read_only_variable() {
        let mut env = Env::new_virtual();
        env.builtins.insert("echo", echo_builtin());
        let function = Function::new(
            "foo",
            function_body_impl("{ echo; }"),
            Location::dummy("dummy"),
        );
        env.functions.define(function).unwrap();
        let mut var = env.variables.get_or_new("x", Scope::Global);
        var.assign("", None).unwrap();
        var.make_read_only(Location::dummy("readonly"));
        let command: SimpleCommand = "x=hello foo".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_matches!(result, Break(Divert::Interrupt(Some(exit_status))) => {
            assert_ne!(exit_status, ExitStatus::SUCCESS);
        });
    }

    #[test]
    fn xtrace_for_function() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let function = Function::new(
            "foo",
            function_body_impl("for i in; do :; done"),
            Location::dummy("dummy"),
        );
        env.functions.define(function).unwrap();
        env.options.set(yash_env::option::XTrace, On);
        let command: SimpleCommand = "x=hello foo bar <>/dev/null".parse().unwrap();

        _ = command.execute(&mut env).now_or_never().unwrap();
        assert_stderr(&state, |stderr| {
            assert_eq!(stderr, "x=hello foo bar 0<>/dev/null\nfor i in\n");
        });
    }
}
