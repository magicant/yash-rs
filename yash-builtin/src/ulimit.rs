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
//! TODO Elaborate ulimit built-in documentation

use crate::common::{output, report_error, report_simple_failure};
use yash_env::semantics::Field;
use yash_env::system::resource::{rlim_t, Resource};
use yash_env::Env;

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

/// Interpretation of command-line arguments that determine the behavior of the
/// `ulimit` built-in
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Command {
    /// Show the current limits for all resources
    ShowAll(ShowLimitType),
    /// Show the current limit for a specific resource
    ShowOne(Resource, ShowLimitType),
    /// Set the limit for a specific resource
    Set(Resource, SetLimitType, rlim_t),
}

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
    /// Other error
    #[error(transparent)]
    Unknown(std::io::Error),
}

impl Command {
    /// Execute the `ulimit` built-in command.
    ///
    /// If successful, returns the string to be printed to the standard output.
    pub async fn execute(&self, env: &mut Env) -> Result<String, Error> {
        match self {
            Command::ShowAll(_) => todo!(),
            Command::ShowOne(_, _) => todo!(),
            Command::Set(_, _, _) => todo!(),
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
