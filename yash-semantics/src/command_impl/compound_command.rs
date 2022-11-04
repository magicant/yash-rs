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

//! Implementation of the compound command semantics.

use super::Command;
use crate::redir::RedirGuard;
use crate::Handle;
use async_trait::async_trait;
use std::ops::ControlFlow::Continue;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::Env;
use yash_syntax::syntax;

/// Executes the condition of an if/while/until command.
async fn evaluate_condition(env: &mut Env, condition: &syntax::List) -> Result<bool> {
    condition.execute(env).await?;
    Continue(env.exit_status == ExitStatus::SUCCESS)
}

mod case;
mod for_loop;
mod r#if;
mod subshell;
mod while_loop;

/// Executes the compound command.
#[async_trait(?Send)]
impl Command for syntax::FullCompoundCommand {
    async fn execute(&self, env: &mut Env) -> Result {
        let mut env = RedirGuard::new(env);
        match env.perform_redirs(&self.redirs).await {
            Ok(_) => self.command.execute(&mut env).await,
            Err(error) => error.handle(&mut env).await,
        }
    }
}

/// Executes the compound command.
///
/// # Grouping
///
/// A grouping is executed by running the contained list.
///
/// # Subshell
///
/// A subshell is executed by running the contained list in a
/// [subshell](Env::run_in_subshell).
///
/// # For loop
///
/// Executing a for loop starts with expanding the `name` and `values`. If
/// `values` is `None`, it expands to the current positional parameters. Each
/// field resulting from the expansion is assigned to the variable `name`, and
/// in turn, `body` is executed.
///
/// # While loop
///
/// The `condition` is executed first. If its exit status is zero, the `body` is
/// executed. The execution is repeated while the `condition` exit status is
/// zero.
///
/// # Until loop
///
/// The until loop is executed in the same manner as the while loop except that
/// the loop condition is inverted: The execution continues until the
/// `condition` exit status is zero.
///
/// # If conditional construct
///
/// The if command first executes the `condition`. If its exit status is zero,
/// it runs the `body`, and its exit status becomes that of the if command.
/// Otherwise, it executes the `condition` of each elif-then clause until
/// finding a condition that returns an exit status of zero, after which it runs
/// the corresponding `body`. If all the conditions result in a non-zero exit
/// status, it runs the `else` clause, if any. In case the command has no `else`
/// clause, the final exit status will be zero.
///
/// # Case conditional construct
///
/// The "case" command expands the subject word and executes the body of the
/// first item with a pattern matching the word. Each pattern is subjected to
/// word expansion before matching.
///
/// POSIX does not specify the order in which the shell tests multiple patterns
/// in an item. This implementation tries them in the order of appearance.
#[async_trait(?Send)]
impl Command for syntax::CompoundCommand {
    async fn execute(&self, env: &mut Env) -> Result {
        use syntax::CompoundCommand::*;
        match self {
            Grouping(list) => list.execute(env).await,
            Subshell { body, location } => subshell::execute(env, body.clone(), location).await,
            For { name, values, body } => for_loop::execute(env, name, values, body).await,
            While { condition, body } => while_loop::execute_while(env, condition, body).await,
            Until { condition, body } => while_loop::execute_until(env, condition, body).await,
            If {
                condition,
                body,
                elifs,
                r#else,
            } => r#if::execute(env, condition, body, elifs, r#else).await,
            Case { subject, items } => case::execute(env, subject, items).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_executor::block_on;
    use std::ops::ControlFlow::Continue;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::VirtualSystem;

    #[test]
    fn redirecting_compound_command() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let command: syntax::FullCompoundCommand = "{ echo 1; echo 2; } > /file".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);

        let file = state.borrow().file_system.get("/file").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content).unwrap(), "1\n2\n");
        });
    }

    #[test]
    fn redirection_error_prevents_command_execution() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let command: syntax::FullCompoundCommand =
            "{ echo not reached; } < /no/such/file".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::ERROR);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn grouping_executes_list() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let command: syntax::CompoundCommand = "{ return -n 42; }".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(42));
    }
}
