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
use yash_env::Env;
#[cfg(doc)]
use yash_env::SharedSystem;
use yash_env::io::Fd;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
#[cfg(doc)]
use yash_env::stack::Stack;
use yash_syntax::source::Location;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::pretty::MessageBase;

pub mod syntax;

/// Convenience function for constructing an error message and a divert value.
///
/// If the environment is currently executing a built-in
/// ([`Stack::current_builtin`]), an annotation indicating the built-in name is
/// appended to the given message. The message is then converted into a string
/// using [`yash_env::io::message_to_string`] and returned along with an
/// optional divert value.
///
/// The [`Divert`] value indicates whether the caller should divert the
/// execution flow. If the current built-in is a special built-in, the second
/// return value is `Break(Divert::Interrupt(None))`; otherwise, `Continue(())`.
///
/// You should always use this function (or another function defined in this
/// module which calls this function) to construct an error or warning message
/// in a built-in. This ensures that the message contains the built-in name
/// in a unified format.
///
/// Use [`SharedSystem::print_error`] to print the returned message and
/// [`crate::Result::with_exit_status_and_divert`] to return the divert value
/// along with an exit status.
#[must_use = "returned message should be printed"]
pub fn arrange_message_and_divert<'e: 'm, 'm>(
    env: &'e Env,
    mut message: Message<'m>,
) -> (String, yash_env::semantics::Result) {
    let is_special_builtin;

    if let Some(builtin) = env.stack.current_builtin() {
        // Add an annotation indicating the built-in name
        message.annotations.push(Annotation::new(
            AnnotationType::Info,
            format!("executing the {} built-in", builtin.name.value).into(),
            &builtin.name.origin,
        ));
        let source = &builtin.name.origin.code.source;
        source.complement_annotations(&mut message.annotations);

        is_special_builtin = builtin.is_special;
    } else {
        is_special_builtin = false;
    }

    let message = yash_env::io::message_to_string(env, &message);
    let divert = if is_special_builtin {
        Break(Divert::Interrupt(None))
    } else {
        Continue(())
    };
    (message, divert)
}

/// Reports a message with the given exit status.
///
/// This is a convenience function for reporting a message with a specific exit
/// status. The message is converted to a string and [`Divert`] using
/// [`arrange_message_and_divert`], and then printed to the standard error.
/// The returned result contains the given exit status and the divert value.
///
/// When the exit status is [`ExitStatus::FAILURE`] or [`ExitStatus::ERROR`],
/// you can use [`report_failure`] or [`report_error`] instead of this function,
/// respectively.
///
/// This function requires a mutable borrow of the environment to print the
/// message, so it is only usable when the `message` argument does not contain
/// any borrows from the environment. Otherwise, directly call
/// [`arrange_message_and_divert`], which only requires an immutable borrow of
/// the environment, to construct the message and divert value, and then print
/// the message yourself.
///
/// ```
/// # use futures_util::future::FutureExt as _;
/// # use yash_builtin::common::arrange_message_and_divert;
/// # use yash_env::builtin::Result;
/// # use yash_env::semantics::ExitStatus;
/// # use yash_syntax::source::pretty::{Annotation, AnnotationType, Message};
/// # use yash_syntax::syntax::Fd;
/// # async {
/// # let mut env = yash_env::Env::new_virtual();
/// # let message = Message { r#type: AnnotationType::Error, title: "".into(), annotations: vec![], footers: vec![] };
/// let (message, divert) = arrange_message_and_divert(&env, message);
/// env.system.print_error(&message).await;
/// Result::with_exit_status_and_divert(ExitStatus::FAILURE, divert)
/// # }.now_or_never().unwrap();
/// ```
#[inline]
pub async fn report<'a, M>(
    env: &mut Env,
    message: M,
    exit_status: ExitStatus,
) -> yash_env::builtin::Result
where
    M: Into<Message<'a>> + 'a,
{
    async fn inner(
        env: &mut Env,
        message: Message<'_>,
        exit_status: ExitStatus,
    ) -> yash_env::builtin::Result {
        let (message, divert) = arrange_message_and_divert(env, message);
        env.system.print_error(&message).await;
        yash_env::builtin::Result::with_exit_status_and_divert(exit_status, divert)
    }
    inner(env, message.into(), exit_status).await
}

/// Prints a failure message.
///
/// This is a simple shortcut for calling [`report`] with [`ExitStatus::FAILURE`].
#[inline]
pub async fn report_failure<'a, M>(env: &mut Env, message: M) -> yash_env::builtin::Result
where
    M: Into<Message<'a>> + 'a,
{
    report(env, message, ExitStatus::FAILURE).await
}

/// Prints an error message.
///
/// This is a simple shortcut for calling [`report`] with [`ExitStatus::ERROR`].
#[inline]
pub async fn report_error<'a, M>(env: &mut Env, message: M) -> yash_env::builtin::Result
where
    M: Into<Message<'a>> + 'a,
{
    report(env, message, ExitStatus::ERROR).await
}

/// Reports a simple message with the given exit status.
///
/// This function constructs a [`Message`] with the given title and prints it
/// using [`report`]. The message has no annotations except for the built-in
/// name which is added by [`arrange_message_and_divert`].
///
/// When the exit status is [`ExitStatus::FAILURE`] or [`ExitStatus::ERROR`],
/// you can use [`report_simple_failure`] or [`report_simple_error`] instead of
/// this function, respectively.
pub async fn report_simple(
    env: &mut Env,
    title: &str,
    exit_status: ExitStatus,
) -> yash_env::builtin::Result {
    let message = Message {
        r#type: AnnotationType::Error,
        title: title.into(),
        annotations: vec![],
        footers: vec![],
    };
    report(env, message, exit_status).await
}

/// Prints a simple failure message.
///
/// This is a simple shortcut for calling [`report_simple`] with [`ExitStatus::FAILURE`].
pub async fn report_simple_failure(env: &mut Env, title: &str) -> yash_env::builtin::Result {
    report_simple(env, title, ExitStatus::FAILURE).await
}

/// Prints a simple error message.
///
/// This is a simple shortcut for calling [`report_simple`] with [`ExitStatus::ERROR`].
pub async fn report_simple_error(env: &mut Env, title: &str) -> yash_env::builtin::Result {
    report_simple(env, title, ExitStatus::ERROR).await
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
        footers: vec![],
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
                footers: vec![],
            };
            report_failure(env, message).await
        }
    }
}

/// Converts errors to a single message.
///
/// If the given iterator is empty, this function returns `None`. Otherwise,
/// the first error's title is used as the message title. The other errors are
/// added as additional annotations.
#[must_use]
pub fn to_single_message<'a, I, M>(errors: I) -> Option<Message<'a>>
where
    I: IntoIterator<Item = &'a M>,
    M: MessageBase + 'a,
{
    let mut errors = errors.into_iter();
    let first = errors.next()?;
    let mut message = Message::from(first);
    let other_errors = errors.map(MessageBase::main_annotation);
    message.annotations.extend(other_errors);
    Some(message)
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
            footers: vec![],
        }
    }

    #[test]
    fn divert_without_builtin() {
        let env = Env::new_virtual();
        let (_message, divert) = arrange_message_and_divert(&env, dummy_message());
        assert_eq!(divert, Continue(()));
    }

    #[test]
    fn divert_with_special_builtin() {
        let mut env = Env::new_virtual();
        let env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("builtin"),
            is_special: true,
        }));

        let (_message, divert) = arrange_message_and_divert(&env, dummy_message());
        assert_eq!(divert, Break(Divert::Interrupt(None)));
    }

    #[test]
    fn divert_with_non_special_builtin() {
        let mut env = Env::new_virtual();
        let env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("builtin"),
            is_special: false,
        }));

        let (_message, divert) = arrange_message_and_divert(&env, dummy_message());
        assert_eq!(divert, Continue(()));
    }
}
