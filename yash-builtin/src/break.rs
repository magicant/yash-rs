// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Break built-in
//!
//! This module implements the [`break` built-in], which terminates the execution of a loop.
//!
//! # Implementation notes
//!
//! A successful invocation of the built-in returns a [`Result`] containing
//! `Break(Divert::Break(n-1))` as its `divert` field. The caller must pass the
//! value to enclosing loops so that the target loop can handle it.
//!
//! Part of the break built-in implementation is shared with the
//! continue built-in implementation.
//!
//! [`break` built-in]: https://magicant.github.io/yash-rs/builtins/break.html

use crate::common::report::{report_error, report_simple_failure};
use yash_env::Env;
use yash_env::builtin::Result;
use yash_env::semantics::Field;
use yash_env::system::{Fcntl, Isatty, Write};

// pub mod display;
pub mod semantics;
pub mod syntax;

/// Entry point for executing the `break` built-in
///
/// This function uses the [`syntax`] and [`semantics`] modules to execute the built-in.
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> Result
where
    S: Fcntl + Isatty + Write,
{
    match syntax::parse(env, args) {
        Ok(count) => match semantics::run(&env.stack, count) {
            Ok(result) => result,
            Err(e) => report_simple_failure(env, &format!("cannot break: {e}")).await,
        },
        Err(e) => report_error(env, &e).await,
    }
}
