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

mod subshell;

use super::perform_redirs;
use super::Command;
use crate::redir::RedirGuard;
use async_trait::async_trait;
use std::ops::ControlFlow::Continue;
use yash_env::semantics::Result;
use yash_env::Env;
use yash_syntax::syntax;

/// Executes the compound command.
#[async_trait(?Send)]
impl Command for syntax::FullCompoundCommand {
    async fn execute(&self, env: &mut Env) -> Result {
        let mut env = RedirGuard::new(env);
        perform_redirs(&mut env, &self.redirs).await?;
        self.command.execute(&mut env).await
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
/// TODO Elaborate
#[async_trait(?Send)]
impl Command for syntax::CompoundCommand {
    async fn execute(&self, env: &mut Env) -> Result {
        use syntax::CompoundCommand::*;
        match self {
            Grouping(list) => list.execute(env).await,
            Subshell(list) => subshell::execute(env, list).await,
            // TODO execute for loop
            // TODO execute while/until loop
            // TODO execute case
            // TODO execute if
            _ => {
                env.print_error(&format!("Not implemented: {}\n", self))
                    .await;
                Continue(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_executor::block_on;
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
    fn grouping_executes_list() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let command: syntax::CompoundCommand = "{ return -n 42; }".parse().unwrap();
        let result = block_on(command.execute(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(42));
    }
}
