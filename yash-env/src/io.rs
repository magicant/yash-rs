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
use crate::source::Location;
use crate::source::pretty::{Report, ReportType, Snippet};
#[cfg(doc)]
use crate::system::SharedSystem;
use crate::system::{Close, Dup, Fcntl, FdFlag, Isatty, Write};
use annotate_snippets::Renderer;
use std::borrow::Cow;
#[cfg(unix)]
use std::os::unix::io::RawFd;

#[cfg(not(unix))]
type RawFd = i32;

/// File descriptor
///
/// This is the `newtype` pattern applied to [`RawFd`], which is merely a type
/// alias.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Fd(pub RawFd);

impl Fd {
    /// File descriptor for the standard input
    pub const STDIN: Fd = Fd(0);
    /// File descriptor for the standard output
    pub const STDOUT: Fd = Fd(1);
    /// File descriptor for the standard error
    pub const STDERR: Fd = Fd(2);
}

impl From<RawFd> for Fd {
    fn from(raw_fd: RawFd) -> Fd {
        Fd(raw_fd)
    }
}

impl std::fmt::Display for Fd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Minimum file descriptor the shell may occupy for its internal use
///
/// POSIX reserves file descriptors below `MIN_INTERNAL_FD` so the user can use
/// them freely. When the shell needs to open a file descriptor that is
/// invisible to the user, it should be kept at `MIN_INTERNAL_FD` or above.
/// (Hint: A typical way to move a file descriptor is to
/// [`dup`](crate::system::Dup::dup) and [`close`](crate::system::Close::close).
/// You can also use [`move_fd_internal`].)
pub const MIN_INTERNAL_FD: Fd = Fd(10);

/// Moves a file descriptor to be at least [`MIN_INTERNAL_FD`].
///
/// This is a convenience function that duplicates the given `from` FD to be at
/// least `MIN_INTERNAL_FD`, and closes the original `from` FD. The new FD will
/// have the `CLOEXEC` flag set. If `from` is already at least
/// `MIN_INTERNAL_FD`, this function does nothing and returns `from`.
///
/// This function can be used to make sure a file descriptor used by the shell
/// does not conflict with file descriptors used by the user.
/// [`MIN_INTERNAL_FD`] is the minimum file descriptor number the shell may use
/// internally.
///
/// Returns the new file descriptor. On error during duplication, the original
/// `from` FD is still closed, and the error is returned. Errors during closing
/// the original `from` FD are ignored.
pub fn move_fd_internal<S>(system: &S, from: Fd) -> crate::system::Result<Fd>
where
    S: Dup + Close + ?Sized,
{
    if from >= MIN_INTERNAL_FD {
        return Ok(from);
    }

    let new = system.dup(from, MIN_INTERNAL_FD, FdFlag::CloseOnExec.into());
    system.close(from).ok();
    new
}

/// Convenience function for converting a report into a string.
///
/// The returned string may contain ANSI color escape sequences if the given
/// `env` allows it. The string will end with a newline.
///
/// To print the returned string to the standard error, you can use
/// [`SharedSystem::print_error`].
#[must_use]
pub fn report_to_string<S: Isatty>(env: &Env<S>, report: &Report<'_>) -> String {
    let renderer = if env.should_print_error_in_color() {
        Renderer::styled()
    } else {
        Renderer::plain()
    };
    format!("{}\n", renderer.render(&[report.into()]))
}

/// Convenience function for printing a report.
///
/// This function converts the `report` into a string by using
/// [`report_to_string`], and prints the result to the standard error.
pub async fn print_report<T: Isatty + Fcntl + Write>(
    env: &mut Env<SharedSystem<T>>,
    report: &Report<'_>,
) {
    let report_str = report_to_string(env, report);
    env.system.print_error(&report_str).await;
}

/// Convenience function for printing an error message.
///
/// This function constructs a temporary [`Report`] based on the given `title`,
/// `label`, and `location`. The message is printed using [`print_report`].
pub async fn print_error<T: Isatty + Fcntl + Write>(
    env: &mut Env<SharedSystem<T>>,
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
