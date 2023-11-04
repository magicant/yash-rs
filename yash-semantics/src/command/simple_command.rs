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

//! Implementation of the simple command semantics.
//!
//! This module exports some utility functions that are used in implementing the
//! simple command semantics and can be used in other modules. For the execution
//! of simple commands, see the implementation of [`Command`] for
//! [`syntax::SimpleCommand`].

use crate::command::Command;
use crate::command_search::search;
use crate::expansion::expand_words;
use crate::xtrace::XTrace;
use crate::Handle;
use async_trait::async_trait;
use std::ffi::CString;
use std::ops::ControlFlow::Continue;
#[cfg(doc)]
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
#[cfg(doc)]
use yash_env::semantics::Field;
use yash_env::semantics::Result;
#[cfg(doc)]
use yash_env::variable::Context;
use yash_env::variable::Scope;
use yash_env::Env;
use yash_syntax::syntax;
use yash_syntax::syntax::Assign;

/// Executes the simple command.
///
/// # Outline
///
/// The execution starts with the [expansion](crate::expansion) of the command
/// words. Next, the [command search](crate::command_search) is performed to
/// find an execution [target](crate::command_search::Target) named by the first
/// [field](Field) of the expansion results. The target type defines how the
/// target is executed. After the execution, the `ErrExit` option is applied
/// with [`Env::apply_errexit`].
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
/// [volatile](Context::Volatile) variable context and exported. Next, a
/// [regular](Context::Regular) context is
/// [pushed](yash_env::variable::VariableSet::push_context) to allow local
/// variable assignment during the function execution. The remaining fields not
/// used in the command search become positional parameters in the new context.
/// After executing the function body, the contexts are
/// [popped](yash_env::variable::VariableSet::pop_context).
///
/// If the execution results in a [`Divert::Return`], it is consumed, and its
/// associated exit status, if any, is set as the exit status of the simple
/// command.
///
/// ## External utility
///
/// If the target is an external utility, a subshell is created.  Redirections
/// and assignments, if any, are performed in the subshell. The assigned
/// variables are exported. The subshell calls the
/// [`execve`](yash_env::System::execve) function to invoke the external utility
/// with all the fields passed as arguments.
///
/// If `execve` fails with an `ENOEXEC` error, it is re-called with the current
/// executable file so that the restarted shell executes the external utility as
/// a shell script.
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
            execute_absent_target(env, &self.assigns, &self.redirs, exit_status).await
        }?;

        env.apply_errexit()
    }
}

async fn perform_assignments(
    env: &mut Env,
    assigns: &[Assign],
    export: bool,
    xtrace: Option<&mut XTrace>,
) -> Result<Option<ExitStatus>> {
    let scope = if export {
        Scope::Volatile
    } else {
        Scope::Global
    };
    match crate::assign::perform_assignments(env, assigns, scope, export, xtrace).await {
        Ok(exit_status) => Continue(exit_status),
        Err(error) => {
            error.handle(env).await?;
            Continue(None)
        }
    }
}

mod absent;
use absent::execute_absent_target;

mod builtin;
use builtin::execute_builtin;

mod function;
use function::execute_function;

mod external;
use external::execute_external_utility;
pub use external::replace_current_process;
pub use external::to_c_strings;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::return_builtin;
    use futures_util::FutureExt;
    use std::ops::ControlFlow::Break;
    use yash_env::option::Option::ErrExit;
    use yash_env::option::State::On;
    use yash_env::semantics::Divert;

    #[test]
    fn errexit_on_simple_command() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        env.options.set(ErrExit, On);
        let command: syntax::SimpleCommand = "return -n 93".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Exit(None)));
        assert_eq!(env.exit_status, ExitStatus(93));
    }
}
