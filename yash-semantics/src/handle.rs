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

//! Error handlers.

use crate::ExitStatus;
use std::ops::ControlFlow::{Break, Continue};
use yash_env::io::print_message;
use yash_env::semantics::Divert;
use yash_env::Env;

/// Error handler.
///
/// Most errors in the shell are handled by printing an error message to the
/// standard error and returning a non-zero exit status. This trait provides a
/// standard interface for implementing that behavior.
pub trait Handle {
    /// Handles the argument error.
    #[allow(async_fn_in_trait)] // We don't support Send
    async fn handle(&self, env: &mut Env) -> super::Result;
}

/// Prints an error message.
///
/// This implementation handles the error by printing an error message to the
/// standard error and returning `Divert::Interrupt(Some(ExitStatus::ERROR))`.
/// Note that other POSIX-compliant implementations may use different non-zero
/// exit statuses.
impl Handle for yash_syntax::parser::Error {
    async fn handle(&self, env: &mut Env) -> super::Result {
        print_message(env, self).await;
        Break(Divert::Interrupt(Some(ExitStatus::ERROR)))
    }
}

/// Prints an error message and returns a divert result indicating a non-zero
/// exit status.
///
/// This implementation handles the error by printing an error message to the
/// standard error and returning `Divert::Interrupt(Some(ExitStatus::ERROR))`.
/// If the [`ErrExit`] option is set, `Divert::Exit(Some(ExitStatus::ERROR))` is
/// returned instead.
///
/// Note that other POSIX-compliant implementations may use different non-zero
/// exit statuses.
///
/// [`ErrExit`]: yash_env::option::Option::ErrExit
impl Handle for crate::expansion::Error {
    async fn handle(&self, env: &mut Env) -> super::Result {
        print_message(env, self).await;

        if env.errexit_is_applicable() {
            Break(Divert::Exit(Some(ExitStatus::ERROR)))
        } else {
            Break(Divert::Interrupt(Some(ExitStatus::ERROR)))
        }
    }
}

/// Prints an error message and sets the exit status to non-zero.
///
/// This implementation handles a redirection error by printing an error message
/// to the standard error and setting the exit status to [`ExitStatus::ERROR`].
/// Note that other POSIX-compliant implementations may use different non-zero
/// exit statuses.
///
/// This implementation does not return [`Divert::Interrupt`] because a
/// redirection error does not always mean an interrupt. The shell should
/// interrupt only on a redirection error during the execution of a special
/// built-in. The caller is responsible for checking the condition and
/// interrupting accordingly.
impl Handle for crate::redir::Error {
    async fn handle(&self, env: &mut Env) -> super::Result {
        print_message(env, self).await;
        env.exit_status = ExitStatus::ERROR;
        Continue(())
    }
}
