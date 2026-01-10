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

//! Command built-in
//!
//! This module implements the [`command` built-in], which executes a utility
//! bypassing shell functions.
//!
//! [`command` built-in]: https://magicant.github.io/yash-rs/builtins/command.html
//!
//! # Implementation notes
//!
//! The `-p` option depends on [`Sysconf::confstr_path`] to obtain the standard
//! search path. See the source code of [`RealSystem::confstr_path`] for the
//! platforms supported on the real system.
//!
//! The built-in depends on some functions injected into the environment's
//! [`any`](Env::any) storage to perform its operations:
//!
//! - An instance of [`RunFunction`] is required to invoke shell functions.
//! - An instance of [`IsKeyword`] is required to check if an argument word is a
//!   shell reserved word (keyword).
//!
//! If no such instance is found, the built-in will **panic**.
//!
//! The [`type`] built-in is equivalent to the `command` built-in with the `-V`
//! option.
//!
//! [`IsKeyword`]: yash_env::parser::IsKeyword
//! [`RunFunction`]: yash_env::semantics::command::RunFunction
//! [`type`]: crate::type

use crate::common::report::report_error;
use enumset::EnumSet;
use enumset::EnumSetType;
use yash_env::Env;
use yash_env::semantics::Field;
#[cfg(all(doc, unix))]
use yash_env::system::real::RealSystem;
use yash_env::system::resource::SetRlimit;
use yash_env::system::{
    Close, Dup, Exec, Exit, Fcntl, Fork, Fstat, GetCwd, GetPid, IsExecutableFile, Isatty, Open,
    SendSignal, SetPgid, ShellPath, Sigaction, Sigmask, Signals, Sysconf, TcSetPgrp, Wait, Write,
};

/// Category of command name resolution
///
/// Used to specify the acceptable categories in [`Search`].
#[derive(Clone, Copy, Debug, EnumSetType, Eq, Hash, PartialEq)]
#[enumset(no_super_impls)]
#[non_exhaustive]
pub enum Category {
    Alias,
    Builtin,
    ExternalUtility,
    Function,
    Keyword,
}

/// Set of parameters that specify how to resolve a command name
///
/// Used in [`Invoke`] and [`Identify`].
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct Search {
    /// Whether to search for the utility in the standard search path
    ///
    /// If `true`, the built-in searches for the utility in the standard search
    /// path instead of the current `$PATH`. The standard path is obtained from
    /// TBD.
    pub standard_path: bool,

    /// Acceptable categories of the command name resolution
    pub categories: EnumSet<Category>,
}

impl Search {
    /// Creates a new `Search` with the default parameters for [`Invoke`].
    #[must_use]
    pub fn default_for_invoke() -> Self {
        Self {
            standard_path: false,
            categories: Category::Builtin | Category::ExternalUtility,
        }
    }

    /// Creates a new `Search` with the default parameters for [`Identify`].
    #[must_use]
    pub fn default_for_identify() -> Self {
        Self {
            standard_path: false,
            categories: EnumSet::all(),
        }
    }
}

/// Parameters to invoke a utility
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invoke {
    /// Command name and arguments
    pub fields: Vec<Field>,
    /// Search parameters
    pub search: Search,
}

impl Default for Invoke {
    fn default() -> Self {
        Self {
            fields: Vec::default(),
            search: Search::default_for_invoke(),
        }
    }
}

/// Parameters to identify a utility
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Identify {
    /// Command names
    pub names: Vec<Field>,
    /// Search parameters
    pub search: Search,
    /// Whether to print a detailed description
    pub verbose: bool,
}

impl Default for Identify {
    fn default() -> Self {
        Self {
            names: Vec::default(),
            search: Search::default_for_identify(),
            verbose: false,
        }
    }
}

/// Parsed command line arguments of the `command` built-in
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Invokes the utility specified by the operands.
    Invoke(Invoke),
    /// Identifies the type and location of the utility.
    Identify(Identify),
}

impl From<Invoke> for Command {
    fn from(invoke: Invoke) -> Self {
        Self::Invoke(invoke)
    }
}

impl From<Identify> for Command {
    fn from(identify: Identify) -> Self {
        Self::Identify(identify)
    }
}

impl Command {
    pub async fn execute<S>(self, env: &mut Env<S>) -> crate::Result
    where
        S: Close
            + Dup
            + Exec
            + Exit
            + Fcntl
            + Fork
            + Fstat
            + GetCwd
            + GetPid
            + IsExecutableFile
            + Isatty
            + Open
            + SendSignal
            + SetPgid
            + SetRlimit
            + ShellPath
            + Sigaction
            + Sigmask
            + Signals
            + Sysconf
            + TcSetPgrp
            + Wait
            + Write
            + 'static,
    {
        match self {
            Self::Invoke(invoke) => invoke.execute(env).await,
            Self::Identify(identify) => identify.execute(env).await,
        }
    }
}

pub mod identify;
mod invoke;
pub mod search;
pub mod syntax;

/// Entry point of the `command` built-in
///
/// This function parses the arguments into [`Command`] and executes it.
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> crate::Result
where
    S: Close
        + Dup
        + Exec
        + Exit
        + Fcntl
        + Fork
        + Fstat
        + GetCwd
        + GetPid
        + IsExecutableFile
        + Isatty
        + Open
        + SendSignal
        + SetPgid
        + SetRlimit
        + ShellPath
        + Sigaction
        + Sigmask
        + Signals
        + Sysconf
        + TcSetPgrp
        + Wait
        + Write
        + 'static,
{
    match syntax::parse(env, args) {
        Ok(command) => command.execute(env).await,
        Err(error) => report_error(env, &error).await,
    }
}
