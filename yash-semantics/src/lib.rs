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

//! Semantics of the shell language.
//!
//! This crate defines the standard semantics for the shell language. The core
//! of the semantics is word expansion and command execution. They respectively
//! have a corresponding trait that is implemented by syntactic constructs.
//!
//! TODO Elaborate

mod command_impl;

use async_trait::async_trait;
use yash_env::env::Env;

pub use yash_env::exec::*;

/// Syntactic construct that can be executed.
#[async_trait(?Send)]
pub trait Command {
    /// Executes this command.
    ///
    /// TODO Elaborate: The exit status must be updated during execution.
    async fn execute(&self, env: &mut Env) -> Result;
}

/// Result of expansion.
///
/// TODO elaborate
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expansion {
    // TODO define value
}

// TODO Reconsider the name
/// TODO describe
#[async_trait(?Send)]
pub trait Word {
    /// TODO describe
    async fn expand(&self, env: &mut Env) -> Result<Expansion>;
}

// TODO Probably we should implement a read-execute loop in here
