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

use crate::Env;
#[cfg(doc)]
use crate::system::SharedSystem;
use annotate_snippets::Renderer;
use std::borrow::Cow;
use yash_syntax::source::Location;
#[allow(deprecated)]
use yash_syntax::source::pretty::Message;
use yash_syntax::source::pretty::{Report, ReportType, Snippet};
#[doc(no_inline)]
pub use yash_syntax::syntax::Fd;

/// Minimum file descriptor the shell may occupy for its internal use
///
/// POSIX reserves file descriptors below `MIN_INTERNAL_FD` so the user can use
/// them freely. When the shell needs to open a file descriptor that is
/// invisible to the user, it should be kept at `MIN_INTERNAL_FD` or above.
/// (Hint: A typical way to move a file descriptor is to
/// [`dup`](crate::system::System::dup) and
/// [`close`](crate::system::System::close). You can also use
/// [`move_fd_internal`].)
///
/// [`move_fd_internal`]: crate::system::SystemEx::move_fd_internal
pub const MIN_INTERNAL_FD: Fd = Fd(10);

/// Convenience function for converting a report into a string.
///
/// The returned string may contain ANSI color escape sequences if the given
/// `env` allows it. The string will end with a newline.
///
/// To print the returned string to the standard error, you can use
/// [`SharedSystem::print_error`].
#[must_use]
pub fn report_to_string(env: &Env, report: &Report<'_>) -> String {
    let renderer = if env.should_print_error_in_color() {
        Renderer::styled()
    } else {
        Renderer::plain()
    };
    format!("{}\n", renderer.render(&[report.into()]))
}

/// Convenience function for converting an error message into a string.
///
/// The returned string may contain ANSI color escape sequences if the given
/// `env` allows it. The string will end with a newline.
///
/// To print the returned string to the standard error, you can use
/// [`SharedSystem::print_error`].
#[allow(deprecated)]
#[deprecated(note = "Use `report_to_string` instead", since = "0.9.0")]
#[must_use]
pub fn message_to_string(env: &Env, message: &Message<'_>) -> String {
    let group = annotate_snippets::Group::from(message);
    let renderer = if env.should_print_error_in_color() {
        Renderer::styled()
    } else {
        Renderer::plain()
    };
    format!("{}\n", renderer.render(&[group]))
}

/// Convenience function for printing a report.
///
/// This function converts the `report` into a string using
/// [`report_to_string`], and prints the result to the standard error.
pub async fn print_report(env: &mut Env, report: &Report<'_>) {
    let report_str = report_to_string(env, report);
    env.system.print_error(&report_str).await;
}

/// Convenience function for printing an error message.
///
/// This function converts the `error` into a [`Message`] which in turn is
/// converted into a string using [`message_to_string`].
/// The result is printed to the standard error.
#[allow(deprecated)]
#[deprecated(note = "Use `print_report` instead", since = "0.9.0")]
pub async fn print_message<'a, E>(env: &mut Env, error: E)
where
    E: Into<Message<'a>> + 'a,
{
    async fn inner(env: &mut Env, message: Message<'_>) {
        env.system
            .print_error(&message_to_string(env, &message))
            .await
    }
    inner(env, error.into()).await;
}

/// Convenience function for printing an error message.
///
/// This function constructs a temporary [`Message`] based on the given `title`,
/// `label`, and `location`. The message is printed using [`print_message`].
pub async fn print_error(
    env: &mut Env,
    title: Cow<'_, str>,
    label: Cow<'_, str>,
    location: &Location,
) {
    let mut report = Report::new();
    report.r#type = ReportType::Error;
    report.title = title;
    report.snippets = Snippet::with_primary_span(location, label);
    print_report(env, &report).await;
}
