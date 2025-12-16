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

//! Umask built-in
//!
//! This module implements the [`umask` built-in], which shows or sets the file
//! mode creation mask.
//!
//! [`umask` built-in]: https://magicant.github.io/yash-rs/builtins/umask.html

use crate::common::output;
use crate::common::report::report_error;
use yash_env::semantics::Field;
use yash_env::system::Mode;
use yash_env::{Env, System};

pub mod eval;
pub mod format;
pub mod symbol;
pub mod syntax;

/// Interpretation of command-line arguments that determine the behavior of the
/// `umask` built-in
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Command {
    /// Show the current file mode creation mask
    Show { symbolic: bool },
    /// Set the file mode creation mask
    Set(Vec<symbol::Clause>),
}

impl Command {
    /// Creates a `Command::Set` variant from a raw mask.
    ///
    /// This is a convenience method for creating a `Command::Set` variant from
    /// a raw mask. The result contains a single clause that sets the mask to
    /// the given value for all bits.
    #[must_use]
    pub fn set_from_raw_mask(mask: u16) -> Self {
        use symbol::{Action, Clause, Operator, Permission, Who};
        Self::Set(vec![Clause {
            who: Who { mask: 0o777 },
            actions: vec![Action {
                operator: Operator::Set,
                permission: Permission::Literal {
                    // Negate because the Operator applies to the negation of the mask
                    mask: !mask,
                    conditional_executable: false,
                },
            }],
        }])
    }

    /// Executes the `umask` built-in.
    ///
    /// Regardless of the command type, this function performs the following steps:
    ///
    /// 1. Obtain the current mask from the environment. ([`System::umask`])
    /// 1. Compute a new mask to be set. ([`eval::new_mask`])
    /// 1. Set the new mask. ([`System::umask`])
    ///
    /// Returns the string that should be printed to the standard output.
    pub fn execute<S: System>(&self, env: &mut Env<S>) -> String {
        let current = !env.system.umask(Mode::empty()).bits();
        let new_mask = eval::new_mask(current as _, self);
        env.system.umask(Mode::from_bits_retain(!new_mask as _));

        match *self {
            Self::Show { symbolic: false } => format!("{:03o}\n", !new_mask),
            Self::Show { symbolic: true } => {
                let mut output = format::format_symbolic(new_mask);
                output.push('\n');
                output
            }
            Self::Set(_) => String::new(),
        }
    }
}

/// Entry point of the `umask` built-in
pub async fn main<S: System>(env: &mut Env<S>, args: Vec<Field>) -> crate::Result {
    match syntax::parse(env, args) {
        Ok(command) => {
            let result = command.execute(env);
            output(env, &result).await
        }
        Err(e) => report_error(env, &e).await,
    }
}
