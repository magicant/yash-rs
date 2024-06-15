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

#[cfg(doc)]
use crate::system::SharedSystem;
use crate::Env;
use annotate_snippets::Renderer;
use std::borrow::Cow;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;
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

/// Convenience function for converting an error message into a string.
///
/// The returned string may contain ANSI color escape sequences if the given
/// `env` allows it. The string will end with a newline.
///
/// To print the returned string to the standard error, you can use
/// [`SharedSystem::print_error`].
#[must_use]
pub fn message_to_string(env: &Env, message: &Message<'_>) -> String {
    let m = annotate_snippets::Message::from(message);
    let r = if env.should_print_error_in_color() {
        Renderer::styled()
    } else {
        Renderer::plain()
    };
    format!("{}\n", r.render(m))
}

/// Convenience function for printing an error message.
///
/// This function converts the `error` into a [`Message`] which in turn is
/// converted into a string using [`message_to_string`].
/// The result is printed to the standard error.
pub async fn print_message<'a, E>(env: &mut Env, error: E)
where
    E: Into<Message<'a>> + 'a,
{
    async fn inner(env: &mut Env, message: Message<'_>) {
        env.system
            .write_all(Fd::STDERR, message_to_string(env, &message).as_bytes())
            .await
            .ok();
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
    let mut a = vec![Annotation::new(AnnotationType::Error, label, location)];
    location.code.source.complement_annotations(&mut a);
    let message = Message {
        r#type: AnnotationType::Error,
        title,
        annotations: a,
        footers: vec![],
    };
    print_message(env, message).await;
}
