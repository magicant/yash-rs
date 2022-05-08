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
use async_trait::async_trait;
use std::ops::ControlFlow::Continue;
use yash_env::semantics::Result;
use yash_env::Env;
use yash_syntax::syntax;

/// Executes the compound command.
#[async_trait(?Send)]
impl Command for syntax::FullCompoundCommand {
    async fn execute(&self, env: &mut Env) -> Result {
        // TODO Open redirections
        self.command.execute(env).await
    }
}

/// Executes the compound command.
///
/// TODO Elaborate
#[async_trait(?Send)]
impl Command for syntax::CompoundCommand {
    async fn execute(&self, env: &mut Env) -> Result {
        use syntax::CompoundCommand::*;
        match self {
            Grouping(list) => list.execute(env).await,
            _ => {
                // TODO execute subshell
                // TODO execute for loop
                // TODO execute while/until loop
                // TODO execute case
                // TODO execute if
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
    use crate::tests::return_builtin;
    use futures_executor::block_on;
    use yash_env::semantics::ExitStatus;

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
