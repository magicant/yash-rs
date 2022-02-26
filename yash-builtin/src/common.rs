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

//! Common items for implementing built-ins.

use annotate_snippets::display_list::DisplayList;
use annotate_snippets::snippet::Snippet;
use async_trait::async_trait;
use std::ops::ControlFlow::Continue;
use yash_env::io::Fd;
use yash_env::semantics::ExitStatus;
use yash_env::system::Errno;
use yash_syntax::source::pretty::Message;

pub mod arg;

/// Part of the execution environment that allows printing to the standard
/// output.
#[async_trait(?Send)]
pub trait Stdout {
    /// Prints a string to the standard output.
    async fn try_print(&mut self, text: &str) -> Result<(), Errno>;
}

#[async_trait(?Send)]
impl Stdout for yash_env::Env {
    async fn try_print(&mut self, text: &str) -> Result<(), Errno> {
        self.system
            .write_all(Fd::STDOUT, text.as_bytes())
            .await
            .map(drop)
    }
}

#[async_trait(?Send)]
impl Stdout for String {
    async fn try_print(&mut self, text: &str) -> Result<(), Errno> {
        self.push_str(text);
        Ok(())
    }
}

/// Part of the execution environment that allows printing to the standard
/// error.
#[async_trait(?Send)]
pub trait Stderr {
    /// Convenience function that prints the given error message.
    ///
    /// This function prints the `message` to the standard error of this
    /// environment. (The exact format of the printed message is subject to
    /// change.)
    ///
    /// Any errors that may happen writing to the standard error are ignored.
    async fn print_error(&mut self, message: std::fmt::Arguments<'_>);

    /// Convenience function that prints an error message for the given `errno`.
    ///
    /// This function prints `format!("{}: {}\n", message, errno.desc())` to the
    /// standard error of this environment. (The exact format of the printed
    /// message is subject to change.)
    ///
    /// Any errors that may happen writing to the standard error are ignored.
    async fn print_system_error(&mut self, errno: Errno, message: std::fmt::Arguments<'_>) {
        self.print_error(format_args!("{}: {}\n", message, errno.desc()))
            .await
    }
}

#[async_trait(?Send)]
impl Stderr for yash_env::Env {
    async fn print_error(&mut self, message: std::fmt::Arguments<'_>) {
        self.print_error(&format!("{}\n", message)).await
    }
}

#[async_trait(?Send)]
impl Stderr for String {
    async fn print_error(&mut self, message: std::fmt::Arguments<'_>) {
        std::fmt::write(&mut self, message).ok();
    }
}

/// Extension of [`Stdout`] that handles errors.
#[async_trait(?Send)]
pub trait Print {
    /// Prints a string to the standard output.
    ///
    /// If an error occurs while printing, an error message is printed to the
    /// standard error and a non-zero exit status is returned.
    async fn print(&mut self, text: &str) -> ExitStatus;
}

#[async_trait(?Send)]
impl<T: Stdout + Stderr> Print for T {
    async fn print(&mut self, text: &str) -> ExitStatus {
        match self.try_print(text).await {
            Ok(()) => ExitStatus::SUCCESS,
            Err(errno) => {
                self.print_system_error(errno, format_args!("cannot print to the standard output"))
                    .await;
                ExitStatus::FAILURE
            }
        }
    }
}

/// Prints an error message.
///
/// This function prepares a [`Message`] from the given error and prints it to
/// the standard error.
///
/// Returns `(ExitStatus::ERROR, ControlFlow::Continue(()))`.
pub async fn print_error_message<'a, E, F>(
    env: &mut E,
    error: F,
) -> (ExitStatus, yash_env::semantics::Result)
where
    E: Stderr,
    F: 'a,
    Message<'a>: From<F>,
{
    let m = Message::from(error);
    let mut s = Snippet::from(&m);
    s.opt.color = true;
    let l = DisplayList::from(s);
    env.print_error(format_args!("{}\n", l)).await;
    (ExitStatus::ERROR, Continue(()))
}
