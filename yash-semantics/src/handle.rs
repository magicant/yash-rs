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
use crate::Runtime;
use std::ops::ControlFlow::{Break, Continue};
use yash_env::Env;
use yash_env::io::print_report;
use yash_env::semantics::Divert;
use yash_syntax::source::Source;

/// Error handler.
///
/// Most errors in the shell are handled by printing an error message to the
/// standard error and returning a non-zero exit status. This trait provides a
/// standard interface for implementing that behavior.
pub trait Handle<S> {
    /// Handles the argument error.
    #[allow(async_fn_in_trait)] // We don't support Send
    async fn handle(&self, env: &mut Env<S>) -> super::Result;
}

/// Prints an error message.
///
/// This implementation handles the error by printing an error message to the
/// standard error and returning `Divert::Interrupt(Some(exit_status))`, where
/// `exit_status` is [`ExitStatus::ERROR`] if the error cause is a syntax error
/// or the error location is [`Source::DotScript`], or
/// [`ExitStatus::READ_ERROR`] otherwise.
/// Note that other POSIX-compliant implementations may use different non-zero
/// exit statuses instead of `ExitStatus::ERROR`.
impl<S: Runtime> Handle<S> for yash_syntax::parser::Error {
    async fn handle(&self, env: &mut Env<S>) -> super::Result {
        print_report(env, &self.to_report()).await;

        use yash_syntax::parser::ErrorCause::*;
        let exit_status = match (&self.cause, &*self.location.code.source) {
            (Syntax(_), _) | (Io(_), Source::DotScript { .. }) => ExitStatus::ERROR,
            (Io(_), _) => ExitStatus::READ_ERROR,
        };
        Break(Divert::Interrupt(Some(exit_status)))
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
impl<S: Runtime> Handle<S> for crate::expansion::Error {
    async fn handle(&self, env: &mut Env<S>) -> super::Result {
        print_report(env, &self.to_report()).await;

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
impl<S: Runtime> Handle<S> for crate::redir::Error {
    async fn handle(&self, env: &mut Env<S>) -> super::Result {
        print_report(env, &self.to_report()).await;
        env.exit_status = ExitStatus::ERROR;
        Continue(())
    }
}

#[cfg(test)]
mod parser_error_tests {
    use super::*;
    use futures_util::FutureExt as _;
    use yash_syntax::parser::{Error, ErrorCause, SyntaxError};
    use yash_syntax::source::{Code, Location};

    #[test]
    fn handling_syntax_error() {
        let mut env = Env::new_virtual();
        let error = Error {
            cause: ErrorCause::Syntax(SyntaxError::RedundantToken),
            location: Location::dummy("test"),
        };
        let result = error.handle(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
    }

    #[test]
    fn handling_io_error_in_command_file() {
        let mut env = Env::new_virtual();
        let code = Code {
            value: "test".to_string().into(),
            start_line_number: 1.try_into().unwrap(),
            source: Source::CommandFile {
                path: "test".to_string(),
            }
            .into(),
        }
        .into();
        let range = 0..0;
        let location = Location { code, range };
        let cause = ErrorCause::Io(std::io::Error::other("error").into());
        let error = Error { cause, location };

        let result = error.handle(&mut env).now_or_never().unwrap();
        assert_eq!(
            result,
            Break(Divert::Interrupt(Some(ExitStatus::READ_ERROR)))
        );
    }

    #[test]
    fn handling_io_error_in_dot_script() {
        let mut env = Env::new_virtual();
        let code = Code {
            value: "test".to_string().into(),
            start_line_number: 1.try_into().unwrap(),
            source: Source::DotScript {
                name: "test".to_string(),
                origin: Location::dummy("test"),
            }
            .into(),
        }
        .into();
        let range = 0..0;
        let location = Location { code, range };
        let cause = ErrorCause::Io(std::io::Error::other("error").into());
        let error = Error { cause, location };

        let result = error.handle(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
    }
}
