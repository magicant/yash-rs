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
//!
//! This module contains common traits to manipulate [`yash_env::Env`] from
//! built-in implementations. These traits abstract the environment and reduce
//! dependency on it.

use async_trait::async_trait;
use std::ops::ControlFlow::Continue;
use yash_env::io::print_error;
use yash_env::io::Fd;
#[doc(no_inline)]
pub use yash_env::io::Stderr;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::stack::Frame;
use yash_env::stack::Stack;
use yash_env::system::Errno;
use yash_syntax::source::pretty::Message;

pub mod arg;

/// Part of the execution environment that allows examining the name of the
/// currently executing built-in.
pub trait BuiltinName {
    /// Returns the name of the currently-executing built-in.
    #[must_use]
    fn builtin_name(&self) -> &Field;
}

impl BuiltinName for Stack {
    /// Returns the name of the currently-executing built-in.
    ///
    /// This function **panics** if `self` does not contain any `Frame::Builtin`
    /// item.
    fn builtin_name(&self) -> &Field {
        self.iter()
            .filter_map(|frame| {
                if let &Frame::Builtin { ref name } = frame {
                    Some(name)
                } else {
                    None
                }
            })
            .next_back()
            .expect("a Frame::Builtin must be in the stack")
    }
}

impl BuiltinName for yash_env::Env {
    /// Returns the name of the currently-executing built-in.
    ///
    /// This function **panics** if `self.stack` does not contain any
    /// `Frame::Builtin` item.
    fn builtin_name(&self) -> &Field {
        self.stack.builtin_name()
    }
}

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
impl<T: BuiltinName + Stdout + Stderr> Print for T {
    async fn print(&mut self, text: &str) -> ExitStatus {
        match self.try_print(text).await {
            Ok(()) => ExitStatus::SUCCESS,
            Err(errno) => {
                let location = self.builtin_name().origin.clone();
                print_error(
                    self,
                    errno.desc().into(),
                    "cannot print results to the standard output".into(),
                    &location,
                )
                .await;
                ExitStatus::FAILURE
            }
        }
    }
}

/// Prints an error message.
///
/// This function prepares a [`Message`] from the given error and prints it to
/// the standard error using {`yash_env::io::print_message`}.
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
    yash_env::io::print_message(env, error).await;
    (ExitStatus::ERROR, Continue(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_name_in_stack() {
        let name = Field::dummy("my built-in");
        let stack = Stack::from(vec![Frame::Builtin { name }]);
        // TODO Test with a stack containing a frame other than Frame::Builtin
        assert_eq!(stack.builtin_name().value, "my built-in");
    }

    #[test]
    #[should_panic(expected = "a Frame::Builtin must be in the stack")]
    fn builtin_name_not_in_stack() {
        let _ = Stack::from(vec![]).builtin_name();
    }
}
