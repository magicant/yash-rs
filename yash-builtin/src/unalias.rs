// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Unalias built-in
//!
//! The **`unalias`** built-in removes alias definitions.
//!
//! # Synopsis
//!
//! ```sh
//! unalias name…
//! ```
//!
//! ```sh
//! unalias -a
//! ```
//!
//! # Description
//!
//! The unalias built-in removes alias definitions as specified by the operands.
//!
//! # Options
//!
//! The **`-a`** (**`--all`**) option removes all alias definitions.
//!
//! # Operands
//!
//! Each operand must be the name of an alias to remove.
//!
//! # Errors
//!
//! It is an error if an operand names a non-existent alias.
//!
//! # Exit status
//!
//! Zero unless an error occurs.
//!
//! # Portability
//!
//! The unalias built-in is specified in POSIX.
//!
//! Some shells implement some built-in utilities as predefined aliases. Using
//! `unalias -a` may make such built-ins unavailable.

use crate::common::report_error;
use crate::common::report_failure;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::pretty::MessageBase;

/// Parsed command arguments for the `unalias` built-in
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    /// Remove specified aliases
    Remove(Vec<Field>),
    /// Remove all aliases
    RemoveAll,
}

pub mod semantics;
pub mod syntax;

/// Converts a non-empty slice of errors to a message.
///
/// The first error's title is used as the message title. The other errors are
/// added as annotations.
///
/// This is a utility for printing errors returned by [`Command::execute`].
/// The returned message can be passed to [`report_failure`].
#[must_use]
pub fn to_message(errors: &[semantics::Error]) -> Option<Message> {
    let mut message = Message::from(errors.first()?);
    let other_errors = errors[1..].iter().map(MessageBase::main_annotation);
    message.annotations.extend(other_errors);
    Some(message)
}

/// Entry point for executing the `unalias` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    match syntax::parse(env, args) {
        Ok(command) => {
            let errors = command.execute(env);
            match to_message(&errors) {
                None => crate::Result::default(),
                Some(message) => report_failure(env, message).await,
            }
        }
        Err(e) => report_error(env, e.to_message()).await,
    }
}
