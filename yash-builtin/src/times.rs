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

//! Times built-in
//!
//! This module implements the [`times` built-in], which is used to display the
//! accumulated user and system times for the shell and its children.
//!
//! [`times` built-in]: https://magicant.github.io/yash-rs/builtins/times.html

use crate::common::output;
use crate::common::report::report_error;
use crate::common::report::report_simple_failure;
use yash_env::Env;
use yash_env::System;
use yash_env::semantics::Field;

mod format;
mod syntax;

/// Entry point of the `times` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    match syntax::parse(env, args) {
        Ok(()) => match env.system.times() {
            Ok(times) => {
                let result = format::format(&times);
                output(env, &result).await
            }
            Err(error) => {
                report_simple_failure(env, &format!("cannot obtain times: {error}")).await
            }
        },
        Err(error) => report_error(env, &error).await,
    }
}
