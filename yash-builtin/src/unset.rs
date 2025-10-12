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

//! Unset built-in
//!
//! This module implements the [`unset` built-in], which unsets shell variables
//! or functions.
//!
//! [`unset` built-in]: https://magicant.github.io/yash-rs/builtins/unset.html

use crate::Result;
use crate::common::report::report_error;
use yash_env::Env;
use yash_env::semantics::Field;

/// Selection of what to unset
#[derive(Debug, Clone, Copy, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Mode {
    /// Unsets shell variables.
    #[default]
    Variables,

    /// Unsets shell functions.
    Functions,
}

/// Parsed command line arguments
#[derive(Debug, Clone, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Command {
    /// What to unset
    pub mode: Mode,

    /// Names of shell variables or functions to unset
    pub names: Vec<Field>,
}

pub mod semantics;
pub mod syntax;

/// Entry point of the `unset` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    let command = match syntax::parse(env, args) {
        Ok(command) => command,
        Err(e) => return report_error(env, &e).await,
    };

    match command.mode {
        Mode::Variables => match semantics::unset_variables(env, &command.names) {
            Ok(()) => Result::default(),
            Err(errors) => semantics::report_variables_error(env, &errors).await,
        },

        Mode::Functions => match semantics::unset_functions(env, &command.names) {
            Ok(()) => Result::default(),
            Err(errors) => semantics::report_functions_error(env, &errors).await,
        },
    }
}
