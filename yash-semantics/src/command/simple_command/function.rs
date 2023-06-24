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
use crate::command::Command;
use crate::redir::RedirGuard;
use crate::xtrace::print;
use crate::xtrace::trace_fields;
use crate::xtrace::XTrace;
use crate::Handle;
use std::ops::ControlFlow::{Break, Continue};
use std::rc::Rc;
use yash_env::function::Function;
use yash_env::semantics::Divert;
use yash_env::semantics::Field;
use yash_env::semantics::Result;
use yash_env::variable::ContextType;
use yash_env::variable::Value;
use yash_env::Env;
use yash_syntax::syntax::Assign;
use yash_syntax::syntax::Redir;

pub async fn execute_function(
    env: &mut Env,
    function: Rc<Function>,
    assigns: &[Assign],
    fields: Vec<Field>,
    redirs: &[Redir],
) -> Result {
    let env = &mut RedirGuard::new(env);
    let mut xtrace = XTrace::from_options(&env.options);
    if let Err(e) = env.perform_redirs(redirs, xtrace.as_mut()).await {
        return e.handle(env).await;
    };

    let mut outer = env.push_context(ContextType::Volatile);
    perform_assignments(&mut outer, assigns, true, xtrace.as_mut()).await?;

    trace_fields(xtrace.as_mut(), &fields);
    print(&mut outer, xtrace).await;

    let mut inner = outer.push_context(ContextType::Regular);

    // Apply positional parameters
    let mut params = inner.variables.positional_params_mut();
    let mut i = fields.into_iter();
    let field = i.next().unwrap();
    params.last_assigned_location = Some(field.origin);
    params.value = Some(Value::array(i.map(|f| f.value)));

    // TODO Update control flow stack
    let result = function.body.execute(&mut inner).await;
    if let Break(Divert::Return(exit_status)) = result {
        if let Some(exit_status) = exit_status {
            inner.exit_status = exit_status;
        }
        Continue(())
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::local_builtin;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::option::State::On;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::variable::Scope;
    use yash_env::variable::Variable;
    use yash_env::VirtualSystem;
    use yash_syntax::source::Location;
    use yash_syntax::syntax;

    #[test]
    fn simple_command_returns_exit_status_from_function() {
        use yash_env::function::HashEntry;
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        env.functions.insert(HashEntry(Rc::new(Function {
            name: "foo".to_string(),
            body: Rc::new("{ return -n 13; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            is_read_only: false,
        })));
        let command: syntax::SimpleCommand = "foo".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(13));
    }

    #[test]
    fn simple_command_applies_redirections_to_function() {
        use yash_env::function::HashEntry;
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.functions.insert(HashEntry(Rc::new(Function {
            name: "foo".to_string(),
            body: Rc::new("{ echo ok; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            is_read_only: false,
        })));
        let command: syntax::SimpleCommand = "foo >/tmp/file".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();

        let file = state.borrow().file_system.get("/tmp/file").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content), Ok("ok\n"));
        });
    }

    #[test]
    fn simple_command_skips_running_function_on_redirection_error() {
        use yash_env::function::HashEntry;
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.functions.insert(HashEntry(Rc::new(Function {
            name: "foo".to_string(),
            body: Rc::new("{ echo ok; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            is_read_only: false,
        })));
        let command: syntax::SimpleCommand = "a=v foo </no/such/file".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::ERROR);
        assert_eq!(env.variables.get("a"), None);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn function_call_consumes_return_without_exit_status() {
        use yash_env::function::HashEntry;
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        env.functions.insert(HashEntry(Rc::new(Function {
            name: "foo".to_string(),
            body: Rc::new("{ return; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            is_read_only: false,
        })));
        env.exit_status = ExitStatus(17);
        let command: syntax::SimpleCommand = "foo".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(17));
    }

    #[test]
    fn function_call_consumes_return_with_exit_status() {
        use yash_env::function::HashEntry;
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        env.functions.insert(HashEntry(Rc::new(Function {
            name: "foo".to_string(),
            body: Rc::new("{ return 26; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            is_read_only: false,
        })));
        let command: syntax::SimpleCommand = "foo".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(26));
    }

    #[test]
    fn simple_command_passes_arguments_to_function() {
        use yash_env::function::HashEntry;
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.functions.insert(HashEntry(Rc::new(Function {
            name: "foo".to_string(),
            body: Rc::new("{ echo $1-$2-$3; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            is_read_only: false,
        })));
        let command: syntax::SimpleCommand = "foo bar baz".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "bar-baz-\n"));
    }

    #[test]
    fn simple_command_creates_temporary_context_executing_function() {
        use yash_env::function::HashEntry;
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("local", local_builtin());
        env.functions.insert(HashEntry(Rc::new(Function {
            name: "foo".to_string(),
            body: Rc::new("{ local x=42; echo $x; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            is_read_only: false,
        })));
        let command: syntax::SimpleCommand = "foo".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(env.variables.get("x"), None);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "42\n"));
    }

    #[test]
    fn simple_command_performs_function_assignment_in_temporary_context() {
        use yash_env::function::HashEntry;
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.functions.insert(HashEntry(Rc::new(Function {
            name: "foo".to_string(),
            body: Rc::new("{ echo $x; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            is_read_only: false,
        })));
        let command: syntax::SimpleCommand = "x=hello foo".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(env.variables.get("x"), None);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "hello\n"));
    }

    #[test]
    fn function_fails_on_reassigning_to_read_only_variable() {
        use yash_env::function::HashEntry;
        let mut env = Env::new_virtual();
        env.builtins.insert("echo", echo_builtin());
        env.functions.insert(HashEntry(Rc::new(Function {
            name: "foo".to_string(),
            body: Rc::new("{ echo; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            is_read_only: false,
        })));
        env.variables
            .assign(
                Scope::Global,
                "x".to_string(),
                Variable::new("").make_read_only(Location::dummy("readonly")),
            )
            .unwrap();
        let command: syntax::SimpleCommand = "x=hello foo".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_matches!(result, Break(Divert::Interrupt(Some(exit_status))) => {
            assert_ne!(exit_status, ExitStatus::SUCCESS);
        });
    }

    #[test]
    fn xtrace_for_function() {
        use yash_env::function::HashEntry;
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.functions.insert(HashEntry(Rc::new(Function {
            name: "foo".to_string(),
            body: Rc::new("for i in; do :; done".parse().unwrap()),
            origin: Location::dummy("dummy"),
            is_read_only: false,
        })));
        env.options.set(yash_env::option::XTrace, On);
        let command: syntax::SimpleCommand = "x=hello foo bar <>/dev/null".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();

        assert_stderr(&state, |stderr| {
            assert_eq!(stderr, "x=hello foo bar 0<>/dev/null\nfor i in\n");
        });
    }
}
