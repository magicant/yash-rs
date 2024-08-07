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

//! Ulimit built-in
//!
//! The **`ulimit`** built-in sets or shows system resource limits.
//!
//! # Synopsis
//!
//! ```sh
//! ulimit [-SH] [-a|-b|-c|-d|-e|-f|-i|-k|-l|-m|-n|-q|-R|-r|-s|-t|-u|-v|-w|-x] [limit]
//! ```
//!
//! # Description
//!
//! The built-in sets or shows the system resource limits. The limits are
//! specified by the *limit* operand. If the *limit* operand is omitted, the
//! built-in shows the current limits.
//!
//! There are two types of limits for each resource: the soft limit and the hard
//! limit. The soft limit is the value that the kernel enforces for the process.
//! The process can increase the soft limit up to the hard limit. The hard limit
//! is the maximum value that the soft limit can be set to. Any process can
//! decrease the hard limit, but only a process with the appropriate privilege
//! can increase the hard limit.
//!
//! # Options
//!
//! The **`-S`** (**`--soft`**) option sets or shows the soft limit. The
//! **`-H`** (**`--hard`**) option sets or shows the hard limit. If neither option
//! is specified:
//!
//! - both the soft limit and the hard limit are set, or
//! - the soft limit is shown.
//!
//! You can also set both limits at once by specifying both options.
//! However, it is an error to specify both options when the *limit* operand is
//! omitted.
//!
//! ## Resources
//!
//! You use an option to specify the resource to set or show the limit on.
//! Available resources vary depending on the platform, so not all of the
//! following options are universally accepted:
//!
//! - **`-b`** (**`--sbsize`**): [`Resource::SBSIZE`] (bytes)
//! - **`-c`** (**`--core`**): [`Resource::CORE`] (512-byte blocks)
//! - **`-d`** (**`--data`**): [`Resource::DATA`] (kilobytes)
//! - **`-e`** (**`--nice`**): [`Resource::NICE`] (see below)
//! - **`-f`** (**`--fsize`**): [`Resource::FSIZE`] (512-byte blocks)
//! - **`-i`** (**`--sigpending`**): [`Resource::SIGPENDING`]
//! - **`-k`** (**`--kqueues`**): [`Resource::KQUEUES`]
//! - **`-l`** (**`--memlock`**): [`Resource::MEMLOCK`] (kilobytes)
//! - **`-m`** (**`--rss`**): [`Resource::RSS`] (kilobytes)
//! - **`-n`** (**`--nofile`**): [`Resource::NOFILE`]
//! - **`-q`** (**`--msgqueue`**): [`Resource::MSGQUEUE`]
//! - **`-R`** (**`--rttime`**): [`Resource::RTTIME`] (microseconds)
//! - **`-r`** (**`--rtprio`**): [`Resource::RTPRIO`]
//! - **`-s`** (**`--stack`**): [`Resource::STACK`] (kilobytes)
//! - **`-t`** (**`--cpu`**): [`Resource::CPU`] (seconds)
//! - **`-u`** (**`--nproc`**): [`Resource::NPROC`]
//! - **`-v`** (**`--as`**): [`Resource::AS`] (kilobytes)
//! - **`-w`** (**`--swap`**): [`Resource::SWAP`]
//! - **`-x`** (**`--locks`**): [`Resource::LOCKS`]
//!
//! Limits that are specified as the *limit* operand and are shown in the output
//! are in the unit specified in the parentheses above. For [`Resource::NICE`],
//! The limit value defines the lower bound for the nice value by the formula
//! `nice = 20 - limit`. Note that lower nice values represent higher
//! priorities.
//!
//! To show the limits for all resources, use the **`-a`** (**`--all`**) option.
//! This option cannot be used with the *limit* operand.
//!
//! # Operands
//!
//! The *limit* operand specifies the new limit to set. The value is interpreted
//! as follows:
//!
//! - If the value is a non-negative integer, the limit is set to that value.
//! - If the value is `unlimited`, the limit is set to the maximum value.
//! - If the value is `hard`, the limit is set to the current hard limit.
//! - If the value is `soft`, the limit is set to the current soft limit.
//!
//! # Standard output
//!
//! If the *limit* operand is omitted, the built-in prints the current limit for
//! the specified resource. If the **`-a`** option is effective, the built-in
//! prints the current limits for all resources in a table.
//!
//! # Errors
//!
//! The built-in may fail when:
//!
//! - The specified resource is not supported on the current platform.
//! - The specified soft limit is greater than the hard limit.
//! - The new hard limit is greater than the current hard limit and the user does
//!   not have permission to raise the hard limit.
//! - The specified *limit* operand is out of range.
//!
//! # Exit status
//!
//! The built-in exits with a non-zero status when an error occurs. Otherwise, it
//! exits with zero.
//!
//! # Examples
//!
//! ```sh
//! ulimit -n 1024
//! ulimit -t unlimited
//! ulimit -v hard
//! ulimit -m soft
//! ulimit -a
//! ```
//!
//! # Portability
//!
//! The `ulimit` built-in is defined in POSIX, but only the `-f` option is
//! required. All the other options are extensions. However, many options
//! including `-H`, `-S`, `-a`, `-c`, `-d`, `-n`, `-s`, and `-t` are widely
//! supported in other shells.
//!
//! See the source code for [`Resource::as_raw_type`] to see which resources are
//! supported on which platforms.
//!
//! The behavior differs between shells when both the `-H` and `-S` options are
//! specified. This implementation sets both limits, but the previous versions
//! of yash honored only the last specified option.
//!
//! The `hard` and `soft` values for the *limit* operand are not defined in
//! POSIX.

use crate::common::{output, report_error, report_simple_failure};
use yash_env::semantics::Field;
use yash_env::system::resource::{rlim_t, Resource};
use yash_env::system::Errno;
use yash_env::Env;
use yash_env::System as _;

/// Type of limit to show
///
/// See [`Command`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShowLimitType {
    Soft,
    Hard,
}

/// Type of limit to set
///
/// See [`Command`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SetLimitType {
    Soft,
    Hard,
    Both,
}

/// Value of the limit to set
///
/// See [`Command`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SetLimitValue {
    /// Numeric value (not scaled)
    Number(rlim_t),
    /// No limit
    Unlimited,
    /// Current soft limit
    CurrentSoft,
    /// Current hard limit
    CurrentHard,
}

/// Interpretation of command-line arguments that determine the behavior of the
/// `ulimit` built-in
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Command {
    /// Show the current limits for all resources
    ShowAll(ShowLimitType),
    /// Show the current limit for a specific resource
    ShowOne(Resource, ShowLimitType),
    /// Set the limit for a specific resource
    Set(Resource, SetLimitType, SetLimitValue),
}

mod resource;
pub use resource::ResourceExt;

pub mod set;
pub mod show;
pub mod syntax;

/// Error that may occur in [`Command::execute`]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The specified resource is not supported on the current platform.
    #[error("specified resource not supported on this platform")]
    UnsupportedResource,
    /// The specified soft limit is greater than the hard limit.
    #[error("soft limit exceeds hard limit")]
    SoftLimitExceedsHardLimit,
    /// The new hard limit is greater than the current hard limit and the user
    /// does not have permission to raise the hard limit.
    #[error("no permission to raise hard limit")]
    NoPermissionToRaiseHardLimit,
    /// The specified limit is out of range.
    #[error("limit out of range")]
    Overflow,
    /// Other error
    #[error("unexpected error: {}", .0)]
    Unknown(Errno),
}

impl Command {
    /// Execute the `ulimit` built-in command.
    ///
    /// If successful, returns the string to be printed to the standard output.
    pub async fn execute(&self, env: &mut Env) -> Result<String, Error> {
        let getrlimit = |resource| env.system.getrlimit(resource);
        match self {
            Command::ShowAll(limit_type) => Ok(show::show_all(getrlimit, *limit_type)),
            Command::ShowOne(resource, limit_type) => {
                show::show_one(getrlimit, *resource, *limit_type)
            }
            Command::Set(resource, limit_type, limit) => {
                set::set(&mut env.system, *resource, *limit_type, *limit)?;
                Ok(String::new())
            }
        }
    }
}

/// Executes the `ulimit` built-in.
///
/// This is the main entry point for the `ulimit` built-in.
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    match syntax::parse(env, args) {
        Ok(command) => match command.execute(env).await {
            Ok(result) => output(env, &result).await,
            Err(e) => report_simple_failure(env, &e.to_string()).await,
        },
        Err(e) => report_error(env, &e).await,
    }
}
