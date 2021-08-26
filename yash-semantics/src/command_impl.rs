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

//! Implementations for Command.

use super::Command;
use async_trait::async_trait;
use yash_env::exec::Result;
use yash_env::Env;
use yash_syntax::syntax;

#[async_trait(?Send)]
impl Command for syntax::Command {
    /// Executes the command.
    async fn execute(&self, env: &mut Env) -> Result {
        use syntax::Command::*;
        match self {
            Simple(command) => command.execute(env).await,
            Compound(command) => command.execute(env).await,
            Function(definition) => definition.execute(env).await,
        }
    }
}

#[async_trait(?Send)]
impl Command for syntax::AndOrList {
    async fn execute(&self, env: &mut Env) -> Result {
        self.first.execute(env).await
        // TODO rest
    }
}

#[async_trait(?Send)]
impl Command for syntax::Item {
    async fn execute(&self, env: &mut Env) -> Result {
        self.and_or.execute(env).await
        // TODO async
    }
}

#[async_trait(?Send)]
impl Command for syntax::List {
    /// Executes the list.
    ///
    /// The list is executed by executing each item in sequence. If any item
    /// results in a [`Divert`](yash_env::exec::Divert), the remaining items are
    /// not executed.
    async fn execute(&self, env: &mut Env) -> Result {
        for item in &self.0 {
            item.execute(env).await?
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::return_builtin;
    use futures_executor::block_on;
    use yash_env::exec::Divert;
    use yash_env::exec::ExitStatus;

    #[test]
    fn list_execute_no_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let list: syntax::List = "return -n 1; return -n 2; return -n 4".parse().unwrap();
        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(4));
    }

    #[test]
    fn list_execute_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let list: syntax::List = "return -n 1; return 2; return -n 4".parse().unwrap();
        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Err(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(2));
    }
}
