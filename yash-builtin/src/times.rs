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
//! The **`times`** built-in is used to display the accumulated user and system
//! times for the shell and its children.
//!
//! # Synopsis
//!
//! ```sh
//! times
//! ```
//!
//! # Description
//!
//! The built-in prints the accumulated user and system times for the shell and
//! its children.
//!
//! # Options
//!
//! None.
//!
//! # Operands
//!
//! None.
//!
//! # Standard output
//!
//! Two lines are printed to the standard output, each in the following format:
//!
//! ```text
//! 1m2.345678s 3m4.567890s
//! ```
//!
//! The first field of each line is the user time, and the second field is the
//! system time.
//! The first line shows the times consumed by the shell itself, and the
//! second line shows the times consumed by its children.
//!
//! # Errors
//!
//! It is an error if the times cannot be obtained or the standard output is not
//! writable.
//!
//! # Exit status
//!
//! Zero unless an error occurred.
//!
//! # Portability
//!
//! The `times` built-in is defined in POSIX.
//!
//! POSIX requires each field to be printed with six digits after the decimal
//! point, but many implementations print less. Note that the number of digits
//! does not necessarily indicate the precision of the times.

use crate::common::output;
use crate::common::report_error;
use crate::common::report_simple_failure;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_env::System;

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
