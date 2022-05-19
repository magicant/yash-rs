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

//! Implementation of simple command semantics.

use crate::command_search::search;
use crate::expansion::expand_words;
use crate::redir::RedirGuard;
use crate::Command;
use crate::Handle;
use async_trait::async_trait;
use std::ffi::CString;
use std::ops::ControlFlow::{Break, Continue};
use std::rc::Rc;
use yash_env::builtin::Builtin;
use yash_env::function::Function;
use yash_env::io::print_error;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::semantics::Result;
use yash_env::stack::Frame;
use yash_env::system::Errno;
use yash_env::variable::ContextType;
use yash_env::variable::Scope;
use yash_env::variable::Value;
use yash_env::Env;
use yash_env::System;
use yash_syntax::syntax;
use yash_syntax::syntax::Assign;
use yash_syntax::syntax::Redir;

/// Executes the simple command.
///
/// # Outline
///
/// The execution starts with the [expansion](crate::expansion) of the command
/// words. Next, the [command search](crate::command_search) is performed to
/// find an execution [target](crate::command_search::Target) named by the first
/// [field](Field) of the expansion results. The remainder of the execution
/// differs depending on the target.
///
/// # Target types and their semantics
///
/// ## Absent target
///
/// If no fields resulted from the expansion, there is no target.
///
/// If the simple command has redirections and assignments, they are performed
/// in a new subshell and the current shell environment, respectively.
///
/// If the redirections or assignments contain command substitutions, the [exit
/// status](ExitStatus) of the simple command is taken from that of the last
/// executed command substitution. Otherwise, the exit status will be zero.
///
/// ## Built-in
///
/// If the target is a built-in, the following steps are performed in the
/// current shell environment.
///
/// First, if there are redirections, they are performed.
///
/// Next, if there are assignments, a temporary context is created to contain
/// the assignment results. The context, as well as the assigned variables, are
/// discarded when the execution finishes. If the target is a regular built-in,
/// the variables are exported.
///
/// Lastly, the built-in is executed by calling its body with the remaining
/// fields passed as arguments.
///
/// ## Function
///
/// If the target is a function, redirections are performed in the same way as a
/// regular built-in. Then, assignments are performed in a
/// [volatile](ContextType::Volatile) variable context and exported. Next, a
/// [regular](ContextType::Regular) context is
/// [pushed](yash_env::variable::VariableSet::push_context) to allow local
/// variable assignment during the function execution. The remaining fields not
/// used in the command search become positional parameters in the new context.
/// After executing the function body, the contexts are
/// [popped](yash_env::variable::VariableSet::pop_context).
///
/// ## External utility
///
/// If the target is an external utility, a subshell is created.  Redirections
/// and assignments, if any, are performed in the subshell. The assigned
/// variables are exported. The subshell calls the
/// [`execve`](yash_env::System::execve) function to invoke the external utility
/// with all the fields passed as arguments.
///
/// If `execve` fails with an `ENOEXEC` error, the behavior is TODO TBD.
///
/// ## Target not found
///
/// If the command search could not find a valid target, the execution proceeds
/// in the same manner as an external utility except that it does not call
/// `execve` and performs error handling as if it failed with `ENOENT`.
///
/// # Redirections
///
/// Redirections are performed in the order of appearance. The file descriptors
/// modified by the redirections are restored after the target has finished
/// except for external utilities executed in a subshell.
///
/// # Assignments
///
/// Assignments are performed in the order of appearance. For each assignment,
/// the value is expanded and assigned to the variable.
///
/// # Errors
///
/// ## Expansion errors
///
/// If there is an error during the expansion, the execution aborts with a
/// non-zero [exit status](ExitStatus) after printing an error message to the
/// standard error.
///
/// Expansion errors may also occur when expanding an assignment value or a
/// redirection operand.
///
/// ## Redirection errors
///
/// Any error happening in redirections causes the execution to abort with a
/// non-zero exit status after printing an error message to the standard error.
///
/// ## Assignment errors
///
/// If an assignment tries to overwrite a read-only variable, the execution
/// aborts with a non-zero exit status after printing an error message to the
/// standard error.
///
/// ## External utility invocation failure
///
/// If the external utility could not be called, the subshell exits after
/// printing an error message to the standard error.
///
/// # Portability
///
/// POSIX does not define the exit status when the `execve` system call fails
/// for a reason other than `ENOEXEC`. In this implementation, the exit status
/// is 127 for `ENOENT` and `ENOTDIR` and 126 for others.
///
/// POSIX leaves many aspects of the simple command execution unspecified. The
/// detail semantics may differ in other shell implementations.
#[async_trait(?Send)]
impl Command for syntax::SimpleCommand {
    async fn execute(&self, env: &mut Env) -> Result {
        let (fields, exit_status) = match expand_words(env, &self.words).await {
            Ok(result) => result,
            Err(error) => return error.handle(env).await,
        };

        use crate::command_search::Target::{Builtin, External, Function};
        if let Some(name) = fields.get(0) {
            match search(env, &name.value) {
                Some(Builtin(builtin)) => {
                    execute_builtin(env, builtin, &self.assigns, fields, &self.redirs).await
                }
                Some(Function(function)) => {
                    execute_function(env, function, &self.assigns, fields, &self.redirs).await
                }
                Some(External { path }) => {
                    execute_external_utility(env, path, &self.assigns, fields, &self.redirs).await
                }
                None => {
                    let path = CString::default();
                    execute_external_utility(env, path, &self.assigns, fields, &self.redirs).await
                }
            }
        } else {
            let exit_status = exit_status.unwrap_or_default();
            execute_absent_target(env, &self.assigns, Rc::clone(&self.redirs), exit_status).await
        }
    }
}

async fn perform_redirs(env: &mut RedirGuard<'_>, redirs: &[Redir]) -> Result<Option<ExitStatus>> {
    match env.perform_redirs(&*redirs).await {
        Ok(exit_status) => Continue(exit_status),
        Err(e) => {
            e.handle(env).await?;
            Continue(None)
        }
    }
}

async fn perform_assignments(
    env: &mut Env,
    assigns: &[Assign],
    export: bool,
) -> Result<Option<ExitStatus>> {
    let scope = if export {
        Scope::Volatile
    } else {
        Scope::Global
    };
    match crate::assign::perform_assignments(env, assigns, scope, export).await {
        Ok(exit_status) => Continue(exit_status),
        Err(error) => {
            error.handle(env).await?;
            Continue(None)
        }
    }
}

async fn execute_absent_target(
    env: &mut Env,
    assigns: &[Assign],
    redirs: Rc<Vec<Redir>>,
    exit_status: ExitStatus,
) -> Result {
    // Perform redirections in a subshell
    let redir_exit_status = if let Some(redir) = redirs.first() {
        let first_redir_location = redir.body.operand().location.clone();
        let redir_results = env.run_in_subshell(move |env| {
            Box::pin(async move {
                let env = &mut RedirGuard::new(env);
                let redir_exit_status = perform_redirs(env, &*redirs).await?;
                env.exit_status = redir_exit_status.unwrap_or(exit_status);
                Continue(())
            })
        });
        match redir_results.await {
            Ok(exit_status) => exit_status,
            Err(errno) => {
                print_error(
                    env,
                    "cannot start subshell to perform redirection".into(),
                    errno.desc().into(),
                    &first_redir_location,
                )
                .await;
                return Break(Divert::Interrupt(Some(ExitStatus::ERROR)));
            }
        }
    } else {
        exit_status
    };

    let assignment_exit_status = perform_assignments(env, assigns, false).await?;
    env.exit_status = assignment_exit_status.unwrap_or(redir_exit_status);
    Continue(())
}

async fn execute_builtin(
    env: &mut Env,
    builtin: Builtin,
    assigns: &[Assign],
    mut fields: Vec<Field>,
    redirs: &[Redir],
) -> Result {
    let name = fields.remove(0);
    let env = &mut env.push_frame(Frame::Builtin { name });
    let env = &mut RedirGuard::new(env);
    perform_redirs(env, redirs).await?;

    use yash_env::builtin::Type::*;
    let (exit_status, abort) = match builtin.r#type {
        Special => {
            perform_assignments(&mut **env, assigns, false).await?;
            (builtin.execute)(env, fields).await
        }
        Intrinsic | NonIntrinsic => {
            let mut env = env.push_context(ContextType::Volatile);
            perform_assignments(&mut env, assigns, true).await?;
            (builtin.execute)(&mut env, fields).await
        }
    };

    env.exit_status = exit_status;
    abort
}

async fn execute_function(
    env: &mut Env,
    function: Rc<Function>,
    assigns: &[Assign],
    fields: Vec<Field>,
    redirs: &[Redir],
) -> Result {
    let env = &mut RedirGuard::new(env);
    perform_redirs(env, redirs).await?;

    let mut outer = env.push_context(ContextType::Volatile);
    perform_assignments(&mut outer, assigns, true).await?;

    let mut inner = outer.push_context(ContextType::Regular);

    // Apply positional parameters
    let mut params = inner.variables.positional_params_mut();
    let mut i = fields.into_iter();
    let field = i.next().unwrap();
    params.last_assigned_location = Some(field.origin);
    params.value = Value::Array(i.map(|f| f.value).collect());

    // TODO Update control flow stack
    let result = function.body.execute(&mut inner).await;
    if result == Break(Divert::Return) {
        Continue(())
    } else {
        result
    }
}

async fn execute_external_utility(
    env: &mut Env,
    path: CString,
    assigns: &[Assign],
    fields: Vec<Field>,
    redirs: &[Redir],
) -> Result {
    let name = fields[0].clone();
    let location = name.origin.clone();
    let args = to_c_strings(fields);

    let env = &mut RedirGuard::new(env);
    perform_redirs(env, redirs).await?;

    let mut env = env.push_context(ContextType::Volatile);
    perform_assignments(&mut *env, assigns, true).await?;

    if path.to_bytes().is_empty() {
        print_error(
            &mut *env,
            format!("cannot execute external utility {:?}", name.value).into(),
            "utility not found".into(),
            &name.origin,
        )
        .await;
        env.exit_status = ExitStatus::NOT_FOUND;
        return Continue(());
    }

    let subshell = env.run_in_subshell(move |env| {
        Box::pin(async move {
            env.traps.disable_internal_handlers(&mut env.system).ok();

            let envs = env.variables.env_c_strings();
            let result = env.system.execve(path.as_c_str(), &args, &envs);
            // TODO Prefer into_err to unwrap_err
            let errno = result.unwrap_err();
            // TODO Reopen as shell script on ENOEXEC
            match errno {
                Errno::ENOENT | Errno::ENOTDIR => {
                    env.exit_status = ExitStatus::NOT_FOUND;
                }
                _ => {
                    env.exit_status = ExitStatus::NOEXEC;
                }
            }
            print_error(
                env,
                format!("cannot execute external utility {:?}", path).into(),
                errno.desc().into(),
                &location,
            )
            .await;
            Continue(())
        })
    });

    match subshell.await {
        Ok(exit_status) => {
            env.exit_status = exit_status;
        }
        Err(errno) => {
            print_error(
                &mut *env,
                format!("cannot execute external utility {:?}", name.value).into(),
                errno.desc().into(),
                &name.origin,
            )
            .await;
            env.exit_status = ExitStatus::NOEXEC;
        }
    }

    Continue(())
}

/// Converts fields to C strings.
fn to_c_strings(s: Vec<Field>) -> Vec<CString> {
    // TODO return something rather than dropping null-containing strings
    s.into_iter()
        .filter_map(|f| CString::new(f.value).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use crate::tests::local_builtin;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_executor::block_on;
    use std::cell::RefCell;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::r#virtual::INode;
    use yash_env::variable::Scope;
    use yash_env::variable::Variable;
    use yash_env::VirtualSystem;
    use yash_syntax::source::Location;

    #[test]
    fn simple_command_performs_redirection_with_absent_target() {
        in_virtual_system(|mut env, _pid, state| async move {
            let command: syntax::SimpleCommand = ">/tmp/foo".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
            let file = state.borrow().file_system.get("/tmp/foo").unwrap();
            let file = file.borrow();
            assert_matches!(&file.body, FileBody::Regular { content, .. } => {
                assert_eq!(from_utf8(content), Ok(""));
            });
        });
    }

    #[test]
    fn simple_command_returns_command_substitution_exit_status_from_redirection() {
        in_virtual_system(|mut env, _pid, _state| async move {
            env.builtins.insert("return", return_builtin());
            let command: syntax::SimpleCommand = ">/tmp/foo$(return -n 42)".parse().unwrap();
            command.execute(&mut env).await;
            assert_eq!(env.exit_status, ExitStatus(42));
        });
    }

    #[test]
    fn simple_command_handles_subshell_error_with_absent_target() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let command: syntax::SimpleCommand = ">/tmp/foo".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));

        let stderr = state.borrow().file_system.get("/dev/stderr").unwrap();
        let stderr = stderr.borrow();
        assert_matches!(&stderr.body, FileBody::Regular { content, .. } => {
            assert_ne!(from_utf8(content).unwrap(), "");
        });
    }

    #[test]
    fn simple_command_performs_assignment_with_absent_target() {
        let mut env = Env::new_virtual();
        let command: syntax::SimpleCommand = "a=b".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_eq!(
            env.variables.get("a").unwrap().value,
            Value::Scalar("b".to_string())
        );
    }

    #[test]
    fn simple_command_returns_command_substitution_exit_status_from_assignment() {
        in_virtual_system(|mut env, _pid, _state| async move {
            env.builtins.insert("return", return_builtin());
            let command: syntax::SimpleCommand = "a=$(return -n 12)".parse().unwrap();
            command.execute(&mut env).await;
            assert_eq!(env.exit_status, ExitStatus(12));
        })
    }

    #[test]
    fn simple_command_handles_assignment_error_with_absent_target() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.variables
            .assign(
                Scope::Global,
                "a".to_string(),
                Variable::new("").make_read_only(Location::dummy("ROL")),
            )
            .unwrap();
        let command: syntax::SimpleCommand = "a=b".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));

        let stderr = state.borrow().file_system.get("/dev/stderr").unwrap();
        let stderr = stderr.borrow();
        assert_matches!(&stderr.body, FileBody::Regular { content, .. } => {
            assert_ne!(from_utf8(content).unwrap(), "");
        });
    }

    #[test]
    fn simple_command_returns_exit_status_from_builtin_without_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let command: syntax::SimpleCommand = "return -n 93".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(93));
    }

    #[test]
    fn simple_command_returns_exit_status_from_builtin_with_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let command: syntax::SimpleCommand = "return 37".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Break(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(37));
    }

    #[test]
    fn simple_command_applies_redirections_to_builtin() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let command: syntax::SimpleCommand = "echo hello >/tmp/file".parse().unwrap();
        block_on(command.execute(&mut env));

        let file = state.borrow().file_system.get("/tmp/file").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content), Ok("hello\n"));
        });
    }

    #[test]
    fn simple_command_assigns_permanently_for_special_builtin() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let command: syntax::SimpleCommand = "v=42 return -n 0".parse().unwrap();
        block_on(command.execute(&mut env));
        let v = env.variables.get("v").unwrap();
        assert_eq!(v.value, Value::Scalar("42".to_string()));
        assert!(!v.is_exported);
    }

    #[test]
    fn simple_command_assigns_temporarily_for_regular_builtin() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("local", local_builtin());
        let command: syntax::SimpleCommand = "v=42 local v".parse().unwrap();
        block_on(command.execute(&mut env));
        assert_eq!(env.variables.get("v"), None);

        let file = state.borrow().file_system.get("/dev/stdout").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content), Ok("v=42\n"));
        });
    }

    #[test]
    fn simple_command_pushes_stack_frame_for_builtin() {
        fn builtin_main(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async {
                assert_matches!(&env.stack[..], &[Frame::Builtin { ref name }] => {
                    assert_eq!(name.value, "builtin");
                });
                (ExitStatus(0), Continue(()))
            })
        }

        let mut env = Env::new_virtual();
        env.builtins.insert(
            "builtin",
            Builtin {
                r#type: yash_env::builtin::Type::Intrinsic,
                execute: builtin_main,
            },
        );
        let command: syntax::SimpleCommand = "builtin".parse().unwrap();
        block_on(command.execute(&mut env));
        assert_eq!(env.stack[..], []);
    }

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
        let result = block_on(command.execute(&mut env));
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
        block_on(command.execute(&mut env));

        let file = state.borrow().file_system.get("/tmp/file").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content), Ok("ok\n"));
        });
    }

    #[test]
    fn function_call_consumes_return() {
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
        let result = block_on(command.execute(&mut env));
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
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Continue(()));

        let file = state.borrow().file_system.get("/dev/stdout").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content), Ok("bar-baz-\n"));
        });
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
        block_on(command.execute(&mut env));
        assert_eq!(env.variables.get("x"), None);

        let stdout = state.borrow().file_system.get("/dev/stdout").unwrap();
        let stdout = stdout.borrow();
        assert_matches!(&stdout.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content), Ok("42\n"));
        });
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
        block_on(command.execute(&mut env));
        assert_eq!(env.variables.get("x"), None);

        let stdout = state.borrow().file_system.get("/dev/stdout").unwrap();
        let stdout = stdout.borrow();
        assert_matches!(&stdout.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content), Ok("hello\n"));
        });
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
        let result = block_on(command.execute(&mut env));
        assert_matches!(result, Break(Divert::Interrupt(Some(exit_status))) => {
            assert_ne!(exit_status, ExitStatus::SUCCESS);
        });
    }

    #[test]
    fn simple_command_calls_execve_with_correct_arguments() {
        in_virtual_system(|mut env, _pid, state| async move {
            let mut content = INode::default();
            content.body = FileBody::Regular {
                content: Vec::new(),
                is_native_executable: true,
            };
            content.permissions.0 |= 0o100;
            let content = Rc::new(RefCell::new(content));
            state.borrow_mut().file_system.save("/some/file", content);

            env.variables
                .assign(
                    Scope::Global,
                    "env".to_string(),
                    Variable::new("scalar").export(),
                )
                .unwrap();
            env.variables
                .assign(Scope::Global, "local".to_string(), Variable::new("ignored"))
                .unwrap();

            let command: syntax::SimpleCommand = "var=123 /some/file foo bar".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));

            let state = state.borrow();
            let process = state.processes.values().last().unwrap();
            let arguments = process.last_exec().as_ref().unwrap();
            assert_eq!(arguments.0, CString::new("/some/file").unwrap());
            assert_eq!(
                arguments.1,
                [
                    CString::new("/some/file").unwrap(),
                    CString::new("foo").unwrap(),
                    CString::new("bar").unwrap()
                ]
            );
            let mut envs = arguments.2.clone();
            envs.sort();
            assert_eq!(
                envs,
                [
                    CString::new("env=scalar").unwrap(),
                    CString::new("var=123").unwrap()
                ]
            );
        });
    }

    #[test]
    fn simple_command_returns_exit_status_from_external_utility() {
        in_virtual_system(|mut env, _pid, state| async move {
            let mut content = INode::default();
            content.body = FileBody::Regular {
                content: Vec::new(),
                is_native_executable: true,
            };
            content.permissions.0 |= 0o100;
            let content = Rc::new(RefCell::new(content));
            state.borrow_mut().file_system.save("/some/file", content);

            let command: syntax::SimpleCommand = "/some/file foo bar".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            // In VirtualSystem, execve fails with ENOSYS.
            assert_eq!(env.exit_status, ExitStatus::NOEXEC);
        });
    }

    #[test]
    fn simple_command_returns_127_for_non_existing_file() {
        in_virtual_system(|mut env, _pid, _state| async move {
            let command: syntax::SimpleCommand = "/some/file".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::NOT_FOUND);
        });
    }

    #[test]
    fn simple_command_returns_126_on_exec_failure() {
        in_virtual_system(|mut env, _pid, state| async move {
            let mut content = INode::default();
            content.permissions.0 |= 0o100;
            let content = Rc::new(RefCell::new(content));
            state.borrow_mut().file_system.save("/some/file", content);

            let command: syntax::SimpleCommand = "/some/file".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::NOEXEC);
        });
    }

    #[test]
    fn simple_command_returns_126_on_fork_failure() {
        let mut env = Env::new_virtual();
        let command: syntax::SimpleCommand = "/some/file".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::NOEXEC);
    }

    #[test]
    fn exit_status_is_127_on_command_not_found() {
        let mut env = Env::new_virtual();
        let command: syntax::SimpleCommand = "no_such_command".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::NOT_FOUND);
    }

    #[test]
    fn simple_command_assigns_variables_in_volatile_context_for_external_command() {
        in_virtual_system(|mut env, _pid, _state| async move {
            let command: syntax::SimpleCommand = "a=123 /foo/bar".parse().unwrap();
            command.execute(&mut env).await;
            assert_eq!(env.variables.get("a"), None);
        });
    }

    #[test]
    fn simple_command_performs_redirections_and_assignments_for_target_not_found() {
        in_virtual_system(|mut env, _pid, state| async move {
            // TODO Test with assignment with side-effect: foo=${bar=baz}
            let command: syntax::SimpleCommand =
                "foo=bar no_such_utility >/tmp/file".parse().unwrap();
            command.execute(&mut env).await;
            assert_eq!(env.variables.get("foo"), None);

            let stdout = state.borrow().file_system.get("/tmp/file").unwrap();
            let stdout = stdout.borrow();
            assert_matches!(&stdout.body, FileBody::Regular { content, .. } => {
                assert_eq!(from_utf8(content), Ok(""));
            });
        });
    }
}
