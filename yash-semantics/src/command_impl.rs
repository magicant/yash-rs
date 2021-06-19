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
use yash_env::expansion::Field;
use yash_env::Env;
use yash_syntax::syntax;

#[async_trait(?Send)]
impl Command for syntax::SimpleCommand {
    async fn execute(&self, env: &mut Env) -> Result {
        // TODO expand words correctly
        let fields: Vec<_> = self
            .words
            .iter()
            .map(|w| Field {
                value: w.to_string(),
                origin: w.location.clone(),
            })
            .collect();

        // TODO open redirections
        // TODO expand and perform assignments

        if let Some(name) = fields.get(0) {
            if let Some(builtin) = env.builtins.get(name.value.as_str()) {
                let (_exit_status, abort) = (builtin.execute)(env, fields).await;
                if let Some(abort) = abort {
                    return Err(abort);
                }
            // TOOD Set exit status to $?
            } else {
                use itertools::Itertools;
                println!("{}", fields.iter().format(" "));
                // TODO execute non-built-in utilities
            }
        }
        Ok(()) // TODO proper command search and execution
    }
}

#[async_trait(?Send)]
impl Command for syntax::Command {
    async fn execute(&self, env: &mut Env) -> Result {
        use syntax::Command::*;
        match self {
            Simple(command) => command.execute(env).await,
            #[allow(clippy::unit_arg)]
            Compound(_) | Function(_) => Ok(println!("{}", self)),
            // TODO execute compound command / function definition
        }
    }
}

#[async_trait(?Send)]
impl Command for syntax::Pipeline {
    async fn execute(&self, env: &mut Env) -> Result {
        // TODO correctly execute pipeline
        self.commands
            .get(0)
            .expect("empty pipeline not yet handled")
            .execute(env)
            .await
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

#[cfg(test)]
mod tests {
    // TODO test
}
