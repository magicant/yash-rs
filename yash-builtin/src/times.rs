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

use crate::common::help::{BuiltinHelp, WithHelp, print_help_of_current_builtin};
use crate::common::output;
use crate::common::report::report_error;
use crate::common::report::report_simple_failure;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::system::concurrency::WriteAll;
use yash_env::system::{Isatty, Times};

mod format;
mod syntax;

/// Help information of the `times` built-in
pub static HELP: BuiltinHelp = BuiltinHelp {
    summary: "print accumulated user and system times",
    usage: &["times"],
    options: syntax::OPTION_SPECS.get(),
    manual_page_name: "times",
};

/// Entry point of the `times` built-in
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> crate::Result
where
    S: Times + Isatty + WriteAll,
{
    match syntax::parse(env, args) {
        Ok(WithHelp::Main(())) => match env.system.times() {
            Ok(times) => {
                let result = format::format(&times);
                output(env, &result).await
            }
            Err(error) => {
                report_simple_failure(env, &format!("cannot obtain times: {error}")).await
            }
        },
        Ok(WithHelp::Help) => print_help_of_current_builtin(env).await,
        Err(error) => report_error(env, &error).await,
    }
}
