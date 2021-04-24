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

//! Built-in utilities.
//!
//! TODO Elaborate

mod alias;

pub use self::alias::alias_built_in;
pub use self::alias::alias_built_in_async;
use crate::env::Env;
use crate::expansion::Field;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

pub use yash_core::builtin::*;

/// Creates a new collection containing all the built-ins.
///
/// ```
/// use yash_syntax::builtin::*;
/// let map = built_ins();
/// assert_eq!(map["alias"].r#type, Type::Intrinsic);
/// ```
pub fn built_ins() -> HashMap<&'static str, BuiltIn> {
    fn def(name: &str, r#type: Type, execute: Main) -> (&str, BuiltIn) {
        (name, BuiltIn { r#type, execute })
    }

    [def("alias", Type::Intrinsic, alias_built_in_async)]
        .iter()
        .cloned()
        .collect()
}
