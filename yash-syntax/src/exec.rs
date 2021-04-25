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

//! Command execution.

use crate::env::Env;
use crate::syntax::*;

pub use yash_core::exec::*;

impl SimpleCommand {
    /// Executes this simple command.
    pub async fn execute(&self, env: &mut dyn Env) -> Result {
        let fields: crate::expansion::Result<Vec<_>> =
            self.words.iter().try_fold(vec![], |mut fs, w| {
                fs.extend(w.expand_multiple(env)?);
                Ok(fs)
            });

        // TODO open redirections
        // TODO expand and perform assignments

        let fields = match fields {
            Ok(fields) => fields,
            Err(_) => return Ok(()),
        };
        if let Some(name) = fields.get(0) {
            if let Some(built_in) = env.builtin(&name.value) {
                let (_exit_status, abort) = (built_in.execute)(env, fields).await;
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

impl Command {
    /// Executes this command.
    pub async fn execute(&self, env: &mut dyn Env) -> Result {
        match self {
            Command::Simple(command) => command.execute(env).await,
            #[allow(clippy::unit_arg)]
            Command::Compound(_) | Command::Function(_) => Ok(println!("{}", self)),
            // TODO execute compound command / function definition
        }
    }
}

impl Pipeline {
    /// Executes this pipeline.
    pub async fn execute(&self, env: &mut dyn Env) -> Result {
        // TODO correctly execute pipeline
        self.commands
            .get(0)
            .expect("empty pipeline not yet handled")
            .execute(env)
            .await
    }
}

impl AndOrList {
    /// Executes this and-or list.
    pub async fn execute(&self, env: &mut dyn Env) -> Result {
        self.first.execute(env).await
        // TODO rest
    }
}

impl Item {
    /// Executes this item.
    pub async fn execute(&self, env: &mut dyn Env) -> Result {
        self.and_or.execute(env).await
        // TODO async
    }
}

impl List {
    /// Executes this list.
    pub async fn execute(&self, env: &mut dyn Env) -> Result {
        for item in &self.0 {
            item.execute(env).await?
        }
        Ok(())
    }
}
