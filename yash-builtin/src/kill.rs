// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Kill built-in
//!
//! This module implements the [`kill` built-in], which sends a signal to processes.
//!
//! [`kill` built-in]: https://magicant.github.io/yash-rs/builtins/kill.html

use crate::common::report::report_error;
use yash_env::Env;
use yash_env::semantics::Field;

mod signal;

pub use signal::Signal;

/// Parsed command line arguments
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Sends a signal to processes
    Send {
        /// Signal to send
        signal: Signal,
        /// Parameter that specified the signal, if any
        signal_origin: Option<Field>,
        /// Target processes
        targets: Vec<Field>,
    },
    /// Lists signal names or descriptions
    Print {
        /// Signals to list
        ///
        /// If empty, all signals are listed.
        signals: Vec<(Signal, Field)>,
        /// Whether to print descriptions
        verbose: bool,
    },
}

pub mod print;
pub mod send;
pub mod syntax;

impl Command {
    /// Executes the built-in.
    pub async fn execute(&self, env: &mut Env) -> crate::Result {
        match self {
            Self::Send {
                signal,
                signal_origin,
                targets,
            } => send::execute(env, *signal, signal_origin.as_ref(), targets).await,

            Self::Print { signals, verbose } => print::execute(env, signals, *verbose).await,
        }
    }
}

/// Entry point of the kill built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    match syntax::parse(env, args) {
        Ok(command) => command.execute(env).await,
        Err(error) => report_error(env, &error).await,
    }
}
