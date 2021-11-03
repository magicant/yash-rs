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

//! Trap built-in.
//!
//! TODO Elaborate

use std::future::ready;
use std::future::Future;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::expansion::Field;

/// Part of the shell execution environment the trap built-in depends on.
pub trait Env {}

impl Env for yash_env::Env {}

/// Implementation of the readonly built-in.
pub fn builtin_main_sync<E: Env>(_env: &mut E, _args: Vec<Field>) -> Result {
    todo!()
}

/// Implementation of the trap built-in.
///
/// This function calls [`builtin_main_sync`] and wraps the result in a `Future`.
pub fn builtin_main(
    env: &mut yash_env::Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result>>> {
    Box::pin(ready(builtin_main_sync(env, args)))
}
