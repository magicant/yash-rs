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

//! Type definitions for I/O.

use crate::system::Errno;
use async_trait::async_trait;
#[doc(no_inline)]
pub use yash_syntax::syntax::Fd;

/// Part of the execution environment that allows printing to the standard
/// error.
#[async_trait(?Send)]
pub trait Stderr {
    /// Convenience function that prints the given error message.
    ///
    /// This function prints the `message` to the standard error of this
    /// environment, ignoring any errors that may happen.
    async fn print_error(&mut self, message: &str);

    /// Convenience function that prints an error message for the given `errno`.
    ///
    /// This function prints `format!("{}: {}\n", message, errno.desc())` to the
    /// standard error of this environment. (The exact format of the printed
    /// message is subject to change.)
    ///
    /// Any errors that may happen writing to the standard error are ignored.
    async fn print_system_error(&mut self, errno: Errno, message: std::fmt::Arguments<'_>) {
        self.print_error(&format!("{}: {}\n", message, errno.desc()))
            .await
    }
}

#[async_trait(?Send)]
impl Stderr for crate::Env {
    async fn print_error(&mut self, message: &str) {
        self.print_error(message).await
    }
}

#[async_trait(?Send)]
impl Stderr for String {
    async fn print_error(&mut self, message: &str) {
        self.push_str(message)
    }
}
