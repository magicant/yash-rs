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
use crate::exec::Abort;
use crate::expansion::Field;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

// TODO should be defined somewhere else.
type ExitStatus = u32;

/// Types of built-in utilities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Type {
    /// Special built-in.
    ///
    /// Special built-in utilities are treated differently from regular built-ins.
    /// Especially, special built-ins are found in the first stage of command
    /// search and cannot be overridden by functions or external utilities. Many
    /// errors in special built-ins force the shell to exit.
    Special,

    /// Intrinsic regular built-in.
    ///
    /// Like special built-ins, intrinsic built-ins are not subject to $PATH in
    /// command search; They are always found regardless of whether there is a
    /// corresponding external utility in $PATH. However, intrinsic built-ins can
    /// still be overridden by functions.
    Intrinsic,

    /// Non-intrinsic regular built-in.
    ///
    /// Non-intrinsic built-ins are much like external utilities; They must be
    /// found in $PATH in order to be executed.
    NonIntrinsic,
}

/// Result of built-in utility.
type Result = (ExitStatus, Option<Abort>);

/// Type of functions that implement the behavior of a built-in.
type Main = fn(&mut dyn Env, Vec<Field>) -> Pin<Box<dyn Future<Output = Result>>>;

/// Built-in utility definition.
#[derive(Clone, Copy)]
pub struct BuiltIn {
    /// Type of the built-in.
    pub r#type: Type,
    /// Function that implements the behavior of the built-in.
    pub execute: Main,
}

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
