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

//! Common items for implementing built-ins
//!
//! This module contains some utility functions for printing messages and a
//! submodule for [parsing command line arguments](syntax).

use std::ops::ControlFlow::{Break, Continue};
use yash_env::io::Fd;
use yash_env::io::Stderr;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
#[cfg(doc)]
use yash_env::stack::Stack;
use yash_env::Env;
#[cfg(doc)]
use yash_env::SharedSystem;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;

pub mod syntax;

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
/// This function is only usable when the `message` argument does not contain
/// any references borrowed from the environment. Otherwise, inline the body of
/// this function into the caller:
///
/// ```
/// # use futures_util::future::FutureExt;
/// # use yash_builtin::common::builtin_message_and_divert;
/// # use yash_env::builtin::Result;
/// # use yash_env::io::Stderr;
/// # use yash_env::semantics::ExitStatus;
/// # use yash_syntax::source::pretty::{Annotation, AnnotationType, Message};
/// # async {
/// # let mut env = yash_env::Env::new_virtual();
/// # let message = Message { r#type: AnnotationType::Error, title: "".into(), annotations: vec![] };
/// let (message, divert) = builtin_message_and_divert(&env, message);
/// env.system.print_error(&message).await;
/// Result::with_exit_status_and_divert(ExitStatus::FAILURE, divert)
/// # }.now_or_never().unwrap();
/// ```
#[inline]
pub async fn report_failure<'a, M>(env: &mut Env, message: M) -> yash_env::builtin::Result
where
    M: Into<Message<'a>> + 'a,
{
    let (message, divert) = builtin_message_and_divert(env, message.into());
    env.system.print_error(&message).await;
    yash_env::builtin::Result::with_exit_status_and_divert(ExitStatus::FAILURE, divert)
}

/// Prints an error message.
///
/// This function is only usable when the `message` argument does not contain
/// any references borrowed from the environment. Otherwise, inline the body of
/// this function into the caller:
///
/// ```
/// # use futures_util::future::FutureExt;
/// # use yash_builtin::common::builtin_message_and_divert;
/// # use yash_env::builtin::Result;
/// # use yash_env::io::Stderr;
/// # use yash_env::semantics::ExitStatus;
/// # use yash_syntax::source::pretty::{Annotation, AnnotationType, Message};
/// # async {
/// # let mut env = yash_env::Env::new_virtual();
/// # let message = Message { r#type: AnnotationType::Error, title: "".into(), annotations: vec![] };
/// let (message, divert) = builtin_message_and_divert(&env, message);
/// env.system.print_error(&message).await;
/// Result::with_exit_status_and_divert(ExitStatus::ERROR, divert)
/// # }.now_or_never().unwrap();
/// ```
#[inline]
pub async fn report_error<'a, M>(env: &mut Env, message: M) -> yash_env::builtin::Result
where
    M: Into<Message<'a>> + 'a,
{
    let (message, divert) = builtin_message_and_divert(env, message.into());
    env.system.print_error(&message).await;
    yash_env::builtin::Result::with_exit_status_and_divert(ExitStatus::ERROR, divert)
}

/// Prints a simple error message for a command syntax error.
///
/// This function constructs a [`Message`] with a predefined title and an
/// [`Annotation`] created from the given label and location, and calls
/// [`report_error`].
pub async fn syntax_error(
    env: &mut Env,
    label: &str,
    location: &Location,
) -> yash_env::builtin::Result {
    let annotation = Annotation::new(AnnotationType::Error, label.into(), location);
    let message = Message {
        r#type: AnnotationType::Error,
        title: "command argument syntax error".into(),
        annotations: vec![annotation],
    };
    report_error(env, message).await
}

/// Prints a text to the standard output.
///
/// This function prints the given text to the standard output, and returns
/// the default result. In case of an error, an error message is printed to
/// the standard error and the returned result has exit status
/// [`ExitStatus::FAILURE`]. Any errors that occur while printing the error
/// message are ignored.
pub async fn output(env: &mut Env, content: &str) -> yash_env::builtin::Result {
    match env.system.write_all(Fd::STDOUT, content.as_bytes()).await {
        Ok(_) => Default::default(),

        Err(errno) => {
            let message = Message {
                r#type: AnnotationType::Error,
                title: format!("error printing results to stdout: {errno}").into(),
                annotations: vec![],
            };
            let (message, divert) = builtin_message_and_divert(env, message);
            env.system.print_error(&message).await;
            yash_env::builtin::Result::with_exit_status_and_divert(ExitStatus::FAILURE, divert)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::semantics::Field;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;

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
