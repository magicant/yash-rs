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
use async_trait::async_trait;
use std::ops::ControlFlow::{Break, Continue};
use yash_env::io::print_message;
use yash_env::semantics::Divert;
use yash_env::Env;

/// Error handler.
///
/// Most errors in the shell are handled by printing an error message to the
/// standard error and returning a non-zero exit status. This trait provides a
/// standard interface for implementing that behavior.
#[async_trait(?Send)]
pub trait Handle {
    /// Handles the argument error.
    async fn handle(&self, env: &mut Env) -> super::Result;
}

/// Prints an error message.
///
/// This implementation handles the error by printing an error message to the
/// standard error and returning `Divert::Interrupt(Some(ExitStatus::ERROR))`.
/// Note that other POSIX-compliant implementations may use different non-zero
/// exit statuses.
#[async_trait(?Send)]
impl Handle for yash_syntax::parser::Error {
    async fn handle(&self, env: &mut Env) -> super::Result {
        print_message(env, self).await;
        Break(Divert::Interrupt(Some(ExitStatus::ERROR)))
    }
}

/// Prints an error message and sets the exit status to non-zero.
///
/// This implementation handles the error by printing an error message to the
/// standard error and returning `Divert::Interrupt(Some(ExitStatus::ERROR))`.
/// Note that other POSIX-compliant implementations may use different non-zero
/// exit statuses.
#[async_trait(?Send)]
impl Handle for crate::expansion::Error {
    async fn handle(&self, env: &mut Env) -> super::Result {
        print_message(env, self).await;
        Break(Divert::Interrupt(Some(ExitStatus::ERROR)))
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
#[async_trait(?Send)]
impl Handle for crate::redir::Error {
    async fn handle(&self, env: &mut Env) -> super::Result {
        print_message(env, self).await;
        env.exit_status = ExitStatus::ERROR;
        Continue(())
    }
}
