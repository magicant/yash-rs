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

//! True built-in
//!
//! This module implements the [`true` built-in], which does nothing, successfully.
//!
//! [`true` built-in]: https://magicant.github.io/yash-rs/builtins/true.html

use crate::Result;
use yash_env::Env;
use yash_env::semantics::Field;

/// Executes the `true` built-in.
///
/// This is the main entry point for the `true` built-in.
pub async fn main<S>(_env: &mut Env<S>, _args: Vec<Field>) -> Result {
    Result::default()
}
