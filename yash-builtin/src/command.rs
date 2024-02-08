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
//! The **`command`** built-in executes a utility bypassing shell functions.
//! This is useful when you want to execute a utility that has the same name as
//! a shell function. The built-in also has options to search for the location
//! of the utility.
//!
//! # Synopsis
//!
//! ```sh
//! command [-p] name [arguments…]
//! ```
//!
//! ```sh
//! command -v|-V [-p] name
//! ```
//!
//! # Description
//!
//! Without the `-v` or `-V` option, the `command` built-in executes the utility
//! specified by the *name* with the given *arguments*. This is similar to the
//! execution of a simple command, but the shell functions are not searched for
//! the *name*.
//!
//! With the `-v` or `-V` option, the built-in identifies the type of the *name*
//! and, optionally, the location of the utility. The `-v` option prints the
//! pathname of the utility, if found, and the `-V` option prints a more
//! detailed description of the utility.
//!
//! # Options
//!
//! The **`-p`** option causes the built-in to search for the utility in the
//! standard search path instead of the current `$PATH`.
//!
//! The **`-v`** option identifies the type of the command name and prints the
//! pathname of the utility, if found.
//!
//! The **`-V`** option identifies the type of the command name and prints a
//! more detailed description of the utility.
//!
//! # Operands
//!
//! The ***name*** operand specifies the name of the utility to execute or
//! identify. The ***arguments*** are passed to the utility when executing it.
//!
//! # Standard output
//!
//! When the `-v` option is given, the built-in prints the following:
//!
//! - The absolute pathname of the utility, if found in the search path.
//! - The utility name itself, if it is a special built-in, function, or shell
//!   reserved word, hence not subject to search.
//! - A command line that would redefine the alias, if the name is an alias.
//!
//! When the `-V` option is given, the built-in describes the utility in a more
//! descriptive, human-readable format. The exact format is not specified here
//! as it is subject to change.
//!
//! Nothing is printed if the utility is not found.
//!
//! # Errors
//!
//! It is an error if the specified utility is not found or cannot be executed.
//!
//! # Exit status
//!
//! Without the `-v` or `-V` option, the exit status is that of the utility
//! executed. If the utility is not found, the exit status is 127. If the
//! utility is found but cannot be executed, the exit status is 126.
//!
//! With the `-v` or `-V` option, the exit status is 0 if the utility is found
//! and 1 if not found.
//!
//! # Portability
//!
//! POSIX requires that the `name` operand be specified, but many
//! implementations allow it to be omitted, in which case the built-in does
//! nothing.
//!
//! When the utility is not found with the `-v` or `-V` option, some
//! implementations return a non-zero exit status other than 1, especially 127.
//!
//! When the utility is not found with the `-V` option, some implementations
//! print an error message to the standard output while others to the standard
//! error.

use enumset::EnumSet;
use enumset::EnumSetType;
use yash_env::semantics::Field;
use yash_env::Env;

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

/// Entry point of the `command` built-in
///
/// This function parses the arguments into [`Command`] and executes it.
pub async fn main(_env: &mut Env, _args: Vec<Field>) -> crate::Result {
    todo!()
}
