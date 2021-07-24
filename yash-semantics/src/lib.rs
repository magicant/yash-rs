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
pub mod command_search;
mod pipeline;
mod simple_command;

use async_trait::async_trait;
use yash_env::Env;

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

#[cfg(test)]
pub(crate) mod tests {
    use itertools::Itertools;
    use std::future::ready;
    use std::future::Future;
    use std::pin::Pin;
    use yash_env::builtin::Builtin;
    use yash_env::builtin::Type::{NonIntrinsic, Special};
    use yash_env::exec::Divert;
    use yash_env::exec::ExitStatus;
    use yash_env::expansion::Field;
    use yash_env::io::Fd;
    use yash_env::Env;

    fn return_builtin_main(
        _env: &mut Env,
        mut args: Vec<Field>,
    ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result>>> {
        let divert = match args.get(1) {
            Some(field) if field.value == "-n" => {
                args.remove(1);
                None
            }
            _ => Some(Divert::Return),
        };
        let exit_status = match args.get(1) {
            Some(field) => field.value.parse().unwrap_or(2),
            None => 0,
        };
        Box::pin(ready((ExitStatus(exit_status), divert)))
    }

    /// Returns a minimal implementation of the `return` built-in.
    pub fn return_builtin() -> Builtin {
        Builtin {
            r#type: Special,
            execute: return_builtin_main,
        }
    }

    fn echo_builtin_main(
        env: &mut Env,
        args: Vec<Field>,
    ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result>>> {
        let fields = (&args[1..]).iter().map(|f| &f.value).format(" ");
        let message = format!("{}\n", fields);
        let result = match env.system.write_all(Fd::STDOUT, message.as_bytes()) {
            Ok(_) => ExitStatus::SUCCESS,
            Err(_) => ExitStatus::FAILURE,
        };
        Box::pin(ready((result, None)))
    }

    /// Returns a minimal implementation of the `echo` built-in.
    pub fn echo_builtin() -> Builtin {
        Builtin {
            r#type: NonIntrinsic,
            execute: echo_builtin_main,
        }
    }

    fn cat_builtin_main(
        env: &mut Env,
        _args: Vec<Field>,
    ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result>>> {
        fn inner(env: &mut Env) -> nix::Result<()> {
            let mut buffer = [0; 1024];
            loop {
                let count = env.system.read(Fd::STDIN, &mut buffer)?;
                if count == 0 {
                    break Ok(());
                }
                env.system.write_all(Fd::STDOUT, &buffer[..count])?;
            }
        }
        let result = match inner(env) {
            Ok(_) => ExitStatus::SUCCESS,
            Err(_) => ExitStatus::FAILURE,
        };
        Box::pin(ready((result, None)))
    }

    /// Returns a minimal implementation of the `cat` built-in.
    pub fn cat_builtin() -> Builtin {
        Builtin {
            r#type: NonIntrinsic,
            execute: cat_builtin_main,
        }
    }
}
