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

//! Pwd built-in.
//!
//! This module implements the [`pwd` built-in], which prints the working directory path.
//!
//! [`pwd` built-in]: https://magicant.github.io/yash-rs/builtins/pwd.html
//!
//! # Implementation notes
//!
//! The result for the `-P` option is obtained with [`System::getcwd`].

use crate::common::output;
use crate::common::report::{report_error, report_failure};
use yash_env::Env;
use yash_env::builtin::Result;
use yash_env::semantics::Field;
use yash_env::system::System;

/// Choice of the behavior of the built-in
#[derive(Debug, Clone, Copy, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Mode {
    /// The built-in prints the value of `$PWD` if it is
    /// [correct](Env::has_correct_pwd).
    ///
    /// If `$PWD` is not a correct path, the built-in falls back to
    /// [`Physical`](Self::Physical).
    #[default]
    Logical,

    /// The built-in computes the canonical path to the working directory.
    Physical,
}

pub mod semantics;
pub mod syntax;

/// Entry point for executing the `pwd` built-in
///
/// This function uses the [`syntax`] and [`semantics`] modules to execute the built-in.
pub async fn main<S: System>(env: &mut Env<S>, args: Vec<Field>) -> Result {
    match syntax::parse(env, args) {
        Ok(mode) => match semantics::compute(env, mode) {
            Ok(result) => output(env, &result).await,
            Err(e) => report_failure(env, &e).await,
        },
        Err(e) => report_error(env, &e).await,
    }
}
