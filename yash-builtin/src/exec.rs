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

//! Exec built-in
//!
//! The exec built-in replaces the current shell process with an external
//! utility invoked by treating the specified operands as a command. Without
//! operands, the built-in makes redirections applied to it permanent in the
//! current shell process.
//!
//! # Syntax
//!
//! ```sh
//! exec [name [arguments...]]
//! ```
//!
//! # Semantics
//!
//! When invoked with operands, the exec built-in replaces the currently
//! executing shell process with a new process image, regarding the operands as
//! command words to start the external utility. The first operand identifies
//! the utility, and the other operands are passed to the utility as
//! command-line arguments.
//!
//! Without operands, the built-in does not start any utility. Instead, it makes
//! any redirections performed in the calling simple command permanent in the
//! current shell environment.
//!
//! # Options
//!
//! POSIX defines no options for the exec built-in.
//!
//! The following non-portable options are yet to be implemented:
//!
//! - `--as`
//! - `--clear`
//! - `--cloexec`
//! - `--force`
//! - `--help`
//!
//! # Operands
//!
//! The operands are treated as a command to start an external utility.
//! If any operands are given, the first is the utility name, and the others are
//! its arguments.
//!
//! If the utility name contains a slash character, the shell will treat it as a
//! path to the utility.
//! Otherwise, the shell will search `$PATH` for the utility.
//!
//! # Exit status
//!
//! If the external utility is invoked successfully, it replaces the shell
//! executing the built-in, so there is no exit status of the built-in.
//! If the built-in fails to invoke the utility, the exit status will be 126.
//! If there is no utility matching the first operand, the exit status will be
//! 127.
//!
//! If no operands are given, the exit status will be 0.
//!
//! # Portability
//!
//! POSIX does not require the exec built-in to conform to the Utility Syntax
//! Guidelines, which means portable scripts cannot use any options or the `--`
//! separator for the built-in.
//!
//! # Implementation notes
//!
//! This implementation uses [`Result::retain_redirs`] to flag redirections to
//! be made permanent.
//!
//! If an operand is given and the utility cannot be invoked successfully, the
//! built-in returns a [`Result`] having `Divert::Exit` to request the calling
//! shell to exit. This behavior is not explicitly required by POSIX, but it is
//! a common practice among existing shells.

use std::future::Future;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::semantics::Field;
use yash_env::Env;

/// Implements the exec built-in.
pub async fn builtin_body(_env: &mut Env, _args: Vec<Field>) -> Result {
    // TODO Implement exec built-in
    let mut result = Result::default();
    result.retain_redirs();
    result
}

/// Wrapper of [`builtin_body`] that returns the future in a pinned box.
pub fn builtin_main(env: &mut Env, args: Vec<Field>) -> Pin<Box<dyn Future<Output = Result> + '_>> {
    Box::pin(builtin_body(env, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use yash_semantics::ExitStatus;

    #[test]
    fn retains_redirs_without_args() {
        let mut env = Env::new_virtual();
        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result.exit_status(), ExitStatus::SUCCESS);
        assert!(result.should_retain_redirs());
    }
}
