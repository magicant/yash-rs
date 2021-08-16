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
    async fn execute(&self, env: &mut Env) -> Result {
        use syntax::Command::*;
        match self {
            Simple(command) => command.execute(env).await,
            Compound(command) => command.execute(env).await,
            Function(_) => {
                // TODO execute function definition
                env.print_error(&format_args!("Not implemented: {}", self))
                    .await;
                Ok(())
            }
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
    async fn execute(&self, env: &mut Env) -> Result {
        for item in &self.0 {
            item.execute(env).await?
        }
        Ok(())
    }
}
