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
//!
//! This module contains some utility functions for printing error messages and
//! a submodule for [parsing command line arguments](syntax).

use async_trait::async_trait;
use std::ops::ControlFlow::{self, Break, Continue};
use yash_env::io::Fd;
#[doc(no_inline)]
pub use yash_env::io::Stderr;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::stack::Stack;
use yash_env::system::Errno;
use yash_env::Env;
use yash_env::SharedSystem;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;

pub mod syntax;

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
        &self
            .current_builtin()
            .expect("a Frame::Builtin must be in the stack")
            .name
    }

    /// Returns whether the currently executing built-in is considered special.
    ///
    /// This function returns false if `self` does not contain any
    /// `Frame::Builtin` item.
    fn is_executing_special_builtin(&self) -> bool {
        self.current_builtin()
            .map_or(false, |builtin| builtin.is_special)
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
impl Stdout for SharedSystem {
    async fn try_print(&mut self, text: &str) -> Result<(), Errno> {
        self.write_all(Fd::STDOUT, text.as_bytes()).await.map(drop)
    }
}

#[async_trait(?Send)]
impl Stdout for String {
    async fn try_print(&mut self, text: &str) -> Result<(), Errno> {
        self.push_str(text);
        Ok(())
    }
}

/// Trait for types that can be cast to [`Stdout`].
pub trait AsStdout {
    type Stdout: Stdout;
    fn as_stdout(&mut self) -> &mut Self::Stdout;
}

impl AsStdout for SharedSystem {
    type Stdout = SharedSystem;
    fn as_stdout(&mut self) -> &mut Self::Stdout {
        self
    }
}

impl AsStdout for yash_env::Env {
    type Stdout = SharedSystem;
    fn as_stdout(&mut self) -> &mut Self::Stdout {
        &mut self.system
    }
}

impl AsStdout for String {
    type Stdout = String;
    fn as_stdout(&mut self) -> &mut Self::Stdout {
        self
    }
}

impl<'a, 'b> AsStdout for (&'a mut String, &'b mut String) {
    type Stdout = String;
    fn as_stdout(&mut self) -> &mut Self::Stdout {
        self.0
    }
}

/// Trait for types that can be cast to [`Stderr`].
pub trait AsStderr {
    type Stderr: Stderr;
    fn as_stderr(&mut self) -> &mut Self::Stderr;
}

impl AsStderr for SharedSystem {
    type Stderr = SharedSystem;
    fn as_stderr(&mut self) -> &mut Self::Stderr {
        self
    }
}

impl AsStderr for yash_env::Env {
    type Stderr = SharedSystem;
    fn as_stderr(&mut self) -> &mut Self::Stderr {
        &mut self.system
    }
}

/// Extension of [`Stdout`] that handles errors.
#[async_trait(?Send)]
pub trait Print {
    /// Prints a string to the standard output.
    ///
    /// If an error occurs while printing, an error message is printed to the
    /// standard error and a non-zero exit status is returned.
    async fn print(&mut self, text: &str) -> yash_env::builtin::Result;
}

#[async_trait(?Send)]
impl<T: BuiltinEnv + AsStdout + AsStderr> Print for T {
    async fn print(&mut self, text: &str) -> yash_env::builtin::Result {
        match self.as_stdout().try_print(text).await {
            Ok(()) => yash_env::builtin::Result::default(),
            Err(errno) => {
                let message = Message {
                    r#type: AnnotationType::Error,
                    title: format!("error printing results to stdout: {errno}").into(),
                    annotations: vec![],
                };
                print_failure_message(self, message).await
            }
        }
    }
}

/// Prints a message.
///
/// This function prepares a [`Message`] by inserting an annotation indicating
/// the [built-in name](BuiltinEnv::builtin_name), and prints it to the standard
/// error using [`yash_env::io::print_message`].
///
/// Returns [`env.builtin_error()`](BuiltinEnv::builtin_error).
pub async fn print_message<'a, E, M>(env: &mut E, message: M) -> yash_env::semantics::Result
where
    E: BuiltinEnv + AsStderr,
    M: Into<Message<'a>> + 'a,
{
    let builtin_name = env.builtin_name();
    let invocation_location = builtin_name.origin.clone();
    let mut message = message.into();
    message.annotations.push(Annotation::new(
        AnnotationType::Info,
        format!("error occurred in the {} built-in", builtin_name.value).into(),
        &invocation_location,
    ));
    invocation_location
        .code
        .source
        .complement_annotations(&mut message.annotations);

    yash_env::io::print_message(env.as_stderr(), message).await;

    env.builtin_error()
}

/// Converts the given message into a string.
///
/// If the environment is currently executing a built-in
/// ([`Stack::current_builtin`]), an annotation indicating the built-in name is
/// appended to the message. The message is then converted into a string using
/// [`yash_env::io::to_string`].
///
/// This function returns an optional [`Divert`] value indicating whether the
/// caller should divert the execution flow. If the environment is currently
/// executing a special built-in, the divert value is `Divert::Interrupt(None)`;
/// otherwise, `None`.
///
/// Note that this function does not print the message. Use
/// [`SharedSystem::print_error`].
#[must_use]
pub fn builtin_message_and_divert<'e: 'm, 'm>(
    env: &'e Env,
    mut message: Message<'m>,
) -> (String, yash_env::semantics::Result) {
    let is_special_builtin;

    if let Some(builtin) = env.stack.current_builtin() {
        // Add an annotation indicating the built-in name
        message.annotations.push(Annotation::new(
            AnnotationType::Info,
            format!("error occurred in the {} built-in", builtin.name.value).into(),
            &builtin.name.origin,
        ));
        let source = &builtin.name.origin.code.source;
        source.complement_annotations(&mut message.annotations);

        is_special_builtin = builtin.is_special;
    } else {
        is_special_builtin = false;
    }

    let message = yash_env::io::to_string(env, message);
    let divert = if is_special_builtin {
        Break(Divert::Interrupt(None))
    } else {
        Continue(())
    };
    (message, divert)
}

/// Prints a failure message.
///
/// This function uses [`print_message`] and returns a result with exit status
/// [`ExitStatus::FAILURE`].
#[inline]
pub async fn print_failure_message<'a, E, M>(env: &mut E, message: M) -> yash_env::builtin::Result
where
    E: BuiltinEnv + AsStderr,
    M: Into<Message<'a>> + 'a,
{
    let result = print_message(env, message).await;
    yash_env::builtin::Result::with_exit_status_and_divert(ExitStatus::FAILURE, result)
}

/// Prints an error message.
///
/// This function uses [`print_message`] and returns a result with exit status
/// [`ExitStatus::ERROR`].
#[inline]
pub async fn print_error_message<'a, E, M>(env: &mut E, message: M) -> yash_env::builtin::Result
where
    E: BuiltinEnv + AsStderr,
    M: Into<Message<'a>> + 'a,
{
    let result = print_message(env, message).await;
    yash_env::builtin::Result::with_exit_status_and_divert(ExitStatus::ERROR, result)
}

/// Prints a simple failure message.
///
/// This function constructs a [`Message`] from the given title and annotation,
/// and calls [`print_failure_message`].
#[inline]
pub async fn print_simple_failure_message<E>(
    env: &mut E,
    title: &str,
    annotation: Annotation<'_>,
) -> yash_env::builtin::Result
where
    E: BuiltinEnv + AsStderr,
{
    let message = Message {
        r#type: AnnotationType::Error,
        title: title.into(),
        annotations: vec![annotation],
    };
    print_failure_message(env, message).await
}

/// Prints a simple error message.
///
/// This function constructs a [`Message`] from the given title and annotation,
/// and calls [`print_error_message`].
#[inline]
pub async fn print_simple_error_message<E>(
    env: &mut E,
    title: &str,
    annotation: Annotation<'_>,
) -> yash_env::builtin::Result
where
    E: BuiltinEnv + AsStderr,
{
    let message = Message {
        r#type: AnnotationType::Error,
        title: title.into(),
        annotations: vec![annotation],
    };
    print_error_message(env, message).await
}

/// Prints a simple error message for a command syntax error.
///
/// This function calls [`print_simple_error_message`] with a predefined title
/// and an [`Annotation`] constructed with the given label and location.
pub async fn syntax_error<E>(
    env: &mut E,
    label: &str,
    location: &Location,
) -> yash_env::builtin::Result
where
    E: BuiltinEnv + AsStderr,
{
    print_simple_error_message(
        env,
        "command argument syntax error",
        Annotation::new(AnnotationType::Error, label.into(), location),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;

    #[test]
    fn builtin_name_in_stack() {
        let name = Field::dummy("my built-in");
        let is_special = false;
        let stack = Stack::from(vec![Frame::Builtin(Builtin { name, is_special })]);
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
        let stack = Stack::from(vec![Frame::Builtin(Builtin { name, is_special })]);
        assert!(stack.is_executing_special_builtin());
    }

    #[test]
    fn is_executing_special_builtin_false_in_stack() {
        let name = Field::dummy("my built-in");
        let is_special = false;
        let stack = Stack::from(vec![Frame::Builtin(Builtin { name, is_special })]);
        assert!(!stack.is_executing_special_builtin());
    }

    #[test]
    fn is_executing_special_builtin_not_in_stack() {
        assert!(!Stack::from(vec![]).is_executing_special_builtin());
    }

    fn dummy_message() -> Message<'static> {
        Message {
            r#type: AnnotationType::Error,
            title: "foo".into(),
            annotations: vec![],
        }
    }

    #[test]
    fn divert_without_builtin() {
        let env = Env::new_virtual();
        let (_message, divert) = builtin_message_and_divert(&env, dummy_message());
        assert_eq!(divert, Continue(()));
    }

    #[test]
    fn divert_with_special_builtin() {
        let mut env = Env::new_virtual();
        let env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("builtin"),
            is_special: true,
        }));

        let (_message, divert) = builtin_message_and_divert(&env, dummy_message());
        assert_eq!(divert, Break(Divert::Interrupt(None)));
    }

    #[test]
    fn divert_with_non_special_builtin() {
        let mut env = Env::new_virtual();
        let env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("builtin"),
            is_special: false,
        }));

        let (_message, divert) = builtin_message_and_divert(&env, dummy_message());
        assert_eq!(divert, Continue(()));
    }
}
