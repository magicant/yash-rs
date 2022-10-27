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
use std::ops::ControlFlow::{self, Break, Continue};
use yash_env::io::Fd;
#[doc(no_inline)]
pub use yash_env::io::Stderr;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::stack::Frame;
use yash_env::stack::Stack;
use yash_env::system::Errno;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;

pub mod arg;

/// Execution environment extension for examining the currently running
/// built-in.
pub trait BuiltinEnv {
    /// Returns the name of the currently-executing built-in.
    #[must_use]
    fn builtin_name(&self) -> &Field;

    /// Returns whether the currently executing built-in is considered special.
    #[must_use]
    fn is_executing_special_builtin(&self) -> bool;

    /// Returns `ControlFlow` on error in a built-in.
    ///
    /// If [`BuiltinEnv::is_executing_special_builtin`], the result is
    /// `Break(Divert::Interrupt(None))`; otherwise, `Continue(())`.
    #[must_use]
    fn builtin_error(&self) -> ControlFlow<Divert>;
}

impl BuiltinEnv for Stack {
    /// Returns the name of the currently-executing built-in.
    ///
    /// This function **panics** if `self` does not contain any `Frame::Builtin`
    /// item.
    fn builtin_name(&self) -> &Field {
        self.iter()
            .filter_map(|frame| {
                if let &Frame::Builtin { ref name, .. } = frame {
                    Some(name)
                } else {
                    None
                }
            })
            .next_back()
            .expect("a Frame::Builtin must be in the stack")
    }

    /// Returns whether the currently executing built-in is considered special.
    ///
    /// This function returns false if `self` does not contain any
    /// `Frame::Builtin` item.
    fn is_executing_special_builtin(&self) -> bool {
        self.iter()
            .filter_map(|frame| {
                if let &Frame::Builtin { is_special, .. } = frame {
                    Some(is_special)
                } else {
                    None
                }
            })
            .next_back()
            .unwrap_or(false)
    }

    fn builtin_error(&self) -> ControlFlow<Divert> {
        if self.is_executing_special_builtin() {
            Break(Divert::Interrupt(None))
        } else {
            Continue(())
        }
    }
}

impl BuiltinEnv for yash_env::Env {
    /// Returns the name of the currently-executing built-in.
    ///
    /// This function **panics** if `self.stack` does not contain any
    /// `Frame::Builtin` item.
    fn builtin_name(&self) -> &Field {
        self.stack.builtin_name()
    }

    fn is_executing_special_builtin(&self) -> bool {
        self.stack.is_executing_special_builtin()
    }

    fn builtin_error(&self) -> ControlFlow<Divert> {
        self.stack.builtin_error()
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
    async fn print(&mut self, text: &str) -> (ExitStatus, yash_env::semantics::Result);
}

#[async_trait(?Send)]
impl<T: BuiltinEnv + Stdout + Stderr> Print for T {
    async fn print(&mut self, text: &str) -> (ExitStatus, yash_env::semantics::Result) {
        match self.try_print(text).await {
            Ok(()) => (ExitStatus::SUCCESS, Continue(())),
            Err(errno) => {
                let message = Message {
                    r#type: AnnotationType::Error,
                    title: format!("error printing results to stdout: {}", errno).into(),
                    annotations: vec![],
                };
                print_failure_message(self, message).await
            }
        }
    }
}

/// Prints a message.
///
/// This function prepares a [`Message`] from the given error, inserts into it
/// an annotation indicating the [built-in name](BuiltinEnv::builtin_name), and
/// prints it to the standard error using [`yash_env::io::print_message`].
///
/// Returns [`env.builtin_error()`](BuiltinEnv::builtin_error).
pub async fn print_message<'a, E, F>(env: &mut E, error: F) -> yash_env::semantics::Result
where
    E: BuiltinEnv + Stderr,
    F: Into<Message<'a>> + 'a,
{
    let builtin_name = env.builtin_name();
    let invocation_location = builtin_name.origin.clone();
    let mut message = error.into();
    message.annotations.push(Annotation::new(
        AnnotationType::Info,
        format!("error occurred in the {} built-in", builtin_name.value).into(),
        &invocation_location,
    ));
    invocation_location
        .code
        .source
        .complement_annotations(&mut message.annotations);

    yash_env::io::print_message(env, message).await;

    env.builtin_error()
}

/// Prints a failure message.
///
/// This function returns
/// `(ExitStatus::FAILURE, print_message(env, error).await)`.
/// See [`print_message`] for details.
#[inline]
pub async fn print_failure_message<'a, E, F>(
    env: &mut E,
    error: F,
) -> (ExitStatus, yash_env::semantics::Result)
where
    E: BuiltinEnv + Stderr,
    F: Into<Message<'a>> + 'a,
{
    (ExitStatus::FAILURE, print_message(env, error).await)
}

/// Prints an error message.
///
/// This function returns
/// `(ExitStatus::ERROR, print_message(env, error).await)`.
/// See [`print_message`] for details.
#[inline]
pub async fn print_error_message<'a, E, F>(
    env: &mut E,
    error: F,
) -> (ExitStatus, yash_env::semantics::Result)
where
    E: BuiltinEnv + Stderr,
    F: Into<Message<'a>> + 'a,
{
    (ExitStatus::ERROR, print_message(env, error).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_name_in_stack() {
        let name = Field::dummy("my built-in");
        let is_special = false;
        let stack = Stack::from(vec![Frame::Builtin { name, is_special }]);
        // TODO Test with a stack containing a frame other than Frame::Builtin
        assert_eq!(stack.builtin_name().value, "my built-in");
    }

    #[test]
    #[should_panic(expected = "a Frame::Builtin must be in the stack")]
    fn builtin_name_not_in_stack() {
        let _ = Stack::from(vec![]).builtin_name();
    }

    #[test]
    fn is_executing_special_builtin_true_in_stack() {
        let name = Field::dummy("my built-in");
        let is_special = true;
        let stack = Stack::from(vec![Frame::Builtin { name, is_special }]);
        assert!(stack.is_executing_special_builtin());
    }

    #[test]
    fn is_executing_special_builtin_false_in_stack() {
        let name = Field::dummy("my built-in");
        let is_special = false;
        let stack = Stack::from(vec![Frame::Builtin { name, is_special }]);
        assert!(!stack.is_executing_special_builtin());
    }

    #[test]
    fn is_executing_special_builtin_not_in_stack() {
        assert!(!Stack::from(vec![]).is_executing_special_builtin());
    }
}
