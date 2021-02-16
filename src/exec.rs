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

/// Result of command execution that requires stack unwinding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Abort {
    /// Break the current loop.
    Break {
        /// Number of loops to break.
        ///
        /// `0` for breaking the innermost loop, `1` for one-level outer, and so on.
        count: usize,
    },
    /// Continue the current loop.
    Continue,
}

/// Result of command execution.
pub type Result<T = ()> = std::result::Result<T, Abort>;

/// Executable command.
pub trait Execute {
    /// Executes `self`.
    fn execute(&self, env: &mut dyn Env) -> Result;
}

impl Execute for SimpleCommand {
    fn execute(&self, _: &mut dyn Env) -> Result {
        println!("{}", self);
        Ok(()) // TODO implement Execute::execute for SimpleCommand
    }
}

impl Execute for Command {
    fn execute(&self, env: &mut dyn Env) -> Result {
        match self {
            Command::SimpleCommand(command) => command.execute(env),
        }
    }
}

impl Execute for Pipeline {
    fn execute(&self, env: &mut dyn Env) -> Result {
        // TODO correctly execute pipeline
        self.commands
            .get(0)
            .expect("empty pipeline not yet handled")
            .execute(env)
    }
}

impl Execute for AndOrList {
    fn execute(&self, env: &mut dyn Env) -> Result {
        self.first.execute(env)
        // TODO rest
    }
}

impl Execute for Item {
    fn execute(&self, env: &mut dyn Env) -> Result {
        self.and_or.execute(env)
        // TODO async
    }
}

impl Execute for List {
    fn execute(&self, env: &mut dyn Env) -> Result {
        self.items.iter().try_for_each(|i| i.execute(env))
    }
}
