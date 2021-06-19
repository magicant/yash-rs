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

//! Type definitions for built-in utilities.
//!
//! This module provides data types for defining built-in utilities.
//!
//! Note that concrete implementations of built-ins are not included in the
//! `yash_env` crate. For implementations of specific built-ins like `cd` and
//! `export`, see the `yash_builtin` crate.

use crate::exec::Divert;
use crate::exec::ExitStatus;
use crate::expansion::Field;
use crate::Env;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

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

/// Result of built-in utility execution.
pub type Result = (ExitStatus, Option<Divert>);

/// Type of functions that implement the behavior of a built-in.
pub type Main = fn(&mut Env, Vec<Field>) -> Pin<Box<dyn Future<Output = Result>>>;

/// Built-in utility definition.
#[derive(Clone, Copy)]
pub struct Builtin {
    /// Type of the built-in.
    pub r#type: Type,
    /// Function that implements the behavior of the built-in.
    pub execute: Main,
}

impl Debug for Builtin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO use finish_non_exhaustive
        f.debug_struct("Builtin")
            .field("type", &self.r#type)
            .finish()
    }
}
