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

use annotate_snippets::display_list::DisplayList;
use annotate_snippets::snippet::Snippet;
use async_trait::async_trait;
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

/// Part of the execution environment that allows printing to the standard
/// error.
#[async_trait(?Send)]
pub trait Stderr {
    /// Convenience function that prints the given error message.
    ///
    /// This function prints the `message` to the standard error of this
    /// environment, ignoring any errors that may happen.
    async fn print_error(&mut self, message: &str);

    /// Returns whether you should include color code in messages printed.
    ///
    /// If this function returns true, you can include escape sequences in the
    /// message passed to [`print_error`](Self::print_error) so that the
    /// terminal shows the message in color. Otherwise, the message should be
    /// plain text.
    ///
    /// The default implementation returns false.
    fn should_print_error_in_color(&self) -> bool {
        false
    }
}

/// Convenience function for printing an error message.
///
/// This function converts the `error` into a [`Message`] which in turn is
/// converted into [`Snippet`] and then [`DisplayList`].
/// The result is printed to the standard error using [`Stderr::print_error`].
pub async fn print_message<'a, S, E>(stderr: &mut S, error: E)
where
    S: Stderr,
    E: Into<Message<'a>> + 'a,
{
    async fn inner(stderr: &mut dyn Stderr, m: Message<'_>) {
        let mut s = Snippet::from(&m);
        s.opt.color = stderr.should_print_error_in_color();
        let f = format!("{}\n", DisplayList::from(s));
        stderr.print_error(&f).await
    }
    inner(stderr, error.into()).await
}

/// Convenience function for printing an error message.
///
/// This function constructs a temporary [`Message`] based on the given `title`,
/// `label`, and `location`. The message is printed using [`print_message`].
pub async fn print_error<S: Stderr>(
    stderr: &mut S,
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
    };
    print_message(stderr, message).await;
}
