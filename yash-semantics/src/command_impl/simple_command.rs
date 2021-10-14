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

use crate::assign::perform_assignments;
use crate::command_search::search;
use crate::expansion::expand_words;
use crate::print_error;
use crate::redir::RedirEnv;
use crate::Command;
use async_trait::async_trait;
use std::ffi::CString;
use std::ops::ControlFlow::Continue;
use std::rc::Rc;
use yash_env::builtin::Builtin;
use yash_env::exec::ExitStatus;
use yash_env::exec::Result;
use yash_env::expansion::Field;
use yash_env::function::Function;
use yash_env::system::Errno;
#[cfg(doc)]
use yash_env::variable::Scope;
use yash_env::Env;
use yash_env::System;
use yash_syntax::syntax;
use yash_syntax::syntax::Assign;
use yash_syntax::syntax::Redir;

#[async_trait(?Send)]
impl Command for syntax::SimpleCommand {
    /// Executes the simple command.
    ///
    /// # Outline
    ///
    /// The execution starts with the [expansion](crate::expansion) of the
    /// command words. Next, the [command search](crate::command_search) is
    /// performed to find an execution [target](crate::command_search::Target)
    /// named by the first [field](Field) of the expansion results. The
    /// remainder of the execution differs depending on the target.
    ///
    /// # Target types and their semantics
    ///
    /// ## Absent target
    ///
    /// If no fields resulted from the expansion, there is no target.
    ///
    /// If the simple command has redirections and assignments, they are
    /// performed in a new subshell and the current shell environment,
    /// respectively.
    ///
    /// If the redirections or assignments contain command substitutions, the
    /// [exit status](ExitStatus) of the simple command is taken from that of
    /// the last executed command substitution. Otherwise, the exit status will
    /// be zero.
    ///
    /// ## Built-in
    ///
    /// If the target is a built-in, the following steps are performed in the
    /// current shell environment.
    ///
    /// First, if there are redirections, they are performed.
    ///
    /// Next, if there are assignments, a temporary context is created to
    /// contain the assignment results. The context, as well as the assigned
    /// variables, are discarded when the execution finishes. If the target is a
    /// regular built-in, the variables are exported.
    ///
    /// Lastly, the built-in is executed by calling its body with the remaining
    /// fields passed as arguments.
    ///
    /// ## Function
    ///
    /// If the target is a function, redirections are performed in the same way
    /// as a regular built-in. Then, a new variable context is
    /// [pushed](Env::push_variable_context) on top of the existing context, and
    /// the assignments are performed [locally](Scope::Local) and exported. The
    /// remaining fields that were not used in the command search become
    /// positional parameters in the new context. After executing the function
    /// body, the context is [popped](Env::pop_variable_context).
    ///
    /// ## External utility
    ///
    /// If the target is an external utility, a subshell is created.
    /// Redirections and assignments, if any, are performed in the subshell. The
    /// assigned variables are exported. The subshell calls the
    /// [`execve`](yash_env::System::execve) function to invoke the external
    /// utility with all the fields passed as arguments.
    ///
    /// If `execve` fails with an `ENOEXEC` error, the behavior is TODO TBD.
    ///
    /// ## Target not found
    ///
    /// If the command search could not find a valid target, the execution
    /// proceeds in the same manner as an external utility except that it does
    /// not call `execve` and performs error handling as if it failed with
    /// `ENOENT`.
    ///
    /// # Redirections
    ///
    /// Redirections are performed in the order of appearance. The file
    /// descriptors modified by the redirections are restored after the target
    /// has finished except for external utilities executed in a subshell.
    ///
    /// # Assignments
    ///
    /// Assignments are performed in the order of appearance. For each
    /// assignment, the value is expanded and assigned to the variable.
    ///
    /// # Errors
    ///
    /// ## Expansion errors
    ///
    /// If there is an error during the expansion, the execution aborts with a
    /// non-zero [exit status](ExitStatus) after printing an error message to
    /// the standard error.
    ///
    /// Expansion errors may also occur when expanding an assignment value or a
    /// redirection operand.
    ///
    /// ## Redirection errors
    ///
    /// Any error happening in redirections causes the execution to abort with a
    /// non-zero exit status after printing an error message to the standard
    /// error.
    ///
    /// ## Assignment errors
    ///
    /// If an assignment tries to overwrite a read-only variable, the execution
    /// aborts with a non-zero exit status after printing an error message to
    /// the standard error.
    ///
    /// ## External utility invocation failure
    ///
    /// If the external utility could not be called, the subshell exits after
    /// printing an error message to the standard error.
    ///
    /// # Portability
    ///
    /// POSIX does not define the exit status when the `execve` system call
    /// fails for a reason other than `ENOEXEC`. In this implementation, the
    /// exit status is 127 for `ENOENT` and `ENOTDIR` and 126 for others.
    ///
    /// POSIX leaves many aspects of the simple command execution unspecified.
    /// The detail semantics may differ in other shell implementations.
    async fn execute(&self, env: &mut Env) -> Result {
        let fields = match expand_words(env, &self.words).await {
            Ok(fields) => fields,
            Err(error) => return error.handle(env).await,
        };

        use crate::command_search::Target::{Builtin, External, Function};
        if let Some(name) = fields.get(0) {
            match search(env, &name.value) {
                Some(Builtin(builtin)) => execute_builtin(env, builtin, fields).await,
                Some(Function(function)) => execute_function(env, function).await,
                Some(External { path }) => {
                    execute_external_utility(env, path, fields, Rc::clone(&self.redirs)).await
                }
                None => {
                    // TODO open redirections
                    // TODO expand and perform assignments
                    // TODO Use pretty::Message and annotate_snippet
                    env.print_error(&format_args!("{}: command not found", name.value))
                        .await;
                    env.exit_status = ExitStatus::NOT_FOUND;
                    Continue(())
                }
            }
        } else {
            execute_absent_target(env, &self.assigns).await
        }
    }
}

async fn execute_absent_target(env: &mut Env, assigns: &[Assign]) -> Result {
    // TODO open redirections

    // TODO Apply last command substitution exit status
    match perform_assignments(env, assigns).await {
        Ok(()) => Continue(()),
        Err(error) => error.handle(env).await,
    }
}

async fn execute_builtin(env: &mut Env, builtin: Builtin, fields: Vec<Field>) -> Result {
    // TODO open redirections
    // TODO expand and perform assignments
    let (exit_status, abort) = (builtin.execute)(env, fields).await;
    env.exit_status = exit_status;
    abort
}

async fn execute_function(env: &mut Env, function: Rc<Function>) -> Result {
    // TODO open redirections
    let mut scope = env.push_variable_context();
    // TODO expand and perform assignments
    // TODO Apply positional parameters
    // TODO Update control flow stack
    function.body.execute(&mut scope).await?;
    // TODO Consume Divert::Return
    Continue(())
}

async fn execute_external_utility(
    env: &mut Env,
    path: CString,
    fields: Vec<Field>,
    redirs: Rc<Vec<Redir>>,
) -> Result {
    let name = fields[0].clone();
    let location = name.origin.clone();
    let args = to_c_strings(fields);
    let envs = env.variables.env_c_strings();
    let result = env
        .run_in_subshell(move |env| {
            Box::pin(async move {
                let mut env = RedirEnv::new(env);
                for redir in &*redirs {
                    if let Err(e) = env.perform_redir(redir).await {
                        e.handle(&mut env).await;
                        return Continue(());
                    }
                }

                // TODO expand and perform assignments

                // TODO Remove signal handlers not set by current traps

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
                    &mut env,
                    format!("cannot execute external command {:?}", path).into(),
                    errno.desc().into(),
                    &location,
                )
                .await;
                Continue(())
            })
        })
        .await;

    match result {
        Ok(exit_status) => {
            env.exit_status = exit_status;
        }
        Err(errno) => {
            print_error(
                env,
                format!("cannot execute external command {:?}", name.value).into(),
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
    use futures_executor::block_on;
    use std::cell::RefCell;
    use std::ops::ControlFlow::Break;
    use std::path::PathBuf;
    use std::rc::Rc;
    use yash_env::exec::Divert;
    use yash_env::system::r#virtual::INode;
    use yash_env::variable::Scope;
    use yash_env::variable::Value;
    use yash_env::variable::Variable;
    use yash_env::VirtualSystem;
    use yash_syntax::source::Location;

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
    fn simple_command_handles_assignment_error_with_absent_target() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.variables
            .assign(
                Scope::Global,
                "a".to_string(),
                Variable {
                    value: Value::Scalar("".to_string()),
                    last_assigned_location: None,
                    is_exported: false,
                    read_only_location: Some(Location::dummy("ROL")),
                },
            )
            .unwrap();
        let command: syntax::SimpleCommand = "a=b".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::ERROR);

        let state = state.borrow();
        let stderr = state.file_system.get("/dev/stderr").unwrap().borrow();
        assert!(!stderr.content.is_empty());
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

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(stdout.content, "42\n".as_bytes());
    }

    #[test]
    fn simple_command_calls_execve_with_correct_arguments() {
        in_virtual_system(|mut env, _pid, state| async move {
            let path = PathBuf::from("/some/file");
            let mut content = INode::default();
            content.permissions.0 |= 0o100;
            content.is_native_executable = true;
            let content = Rc::new(RefCell::new(content));
            state.borrow_mut().file_system.save(path, content);

            env.variables
                .assign(
                    Scope::Global,
                    "env".to_string(),
                    Variable {
                        value: Value::Scalar("scalar".to_string()),
                        last_assigned_location: None,
                        is_exported: true,
                        read_only_location: None,
                    },
                )
                .unwrap();
            env.variables
                .assign(
                    Scope::Global,
                    "local".to_string(),
                    Variable {
                        value: Value::Scalar("ignored".to_string()),
                        last_assigned_location: None,
                        is_exported: false,
                        read_only_location: None,
                    },
                )
                .unwrap();

            let command: syntax::SimpleCommand = "/some/file foo bar".parse().unwrap();
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
            assert_eq!(arguments.2, [CString::new("env=scalar").unwrap()]);
        });
    }

    #[test]
    fn simple_command_returns_exit_status_from_external_utility() {
        in_virtual_system(|mut env, _pid, state| async move {
            let path = PathBuf::from("/some/file");
            let mut content = INode::default();
            content.permissions.0 |= 0o100;
            content.is_native_executable = true;
            let content = Rc::new(RefCell::new(content));
            state.borrow_mut().file_system.save(path, content);

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
            let path = PathBuf::from("/some/file");
            let mut content = INode::default();
            content.permissions.0 |= 0o100;
            let content = Rc::new(RefCell::new(content));
            state.borrow_mut().file_system.save(path, content);

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
}
