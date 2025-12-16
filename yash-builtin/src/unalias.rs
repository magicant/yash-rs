// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Unalias built-in
//!
//! This module implements the [`unalias` built-in], which removes alias definitions.
//!
//! [`unalias` built-in]: https://magicant.github.io/yash-rs/builtins/unalias.html

use crate::common::report::merge_reports;
use crate::common::report::report_error;
use crate::common::report::report_failure;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::system::System;

/// Parsed command arguments for the `unalias` built-in
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    /// Remove specified aliases
    Remove(Vec<Field>),
    /// Remove all aliases
    RemoveAll,
}

pub mod semantics;
pub mod syntax;

/// Entry point for executing the `unalias` built-in
pub async fn main<S: System>(env: &mut Env<S>, args: Vec<Field>) -> crate::Result {
    match syntax::parse(env, args) {
        Ok(command) => {
            let errors = command.execute(env);
            match merge_reports(&errors) {
                None => crate::Result::default(),
                Some(report) => report_failure(env, report).await,
            }
        }
        Err(e) => report_error(env, &e).await,
    }
}
