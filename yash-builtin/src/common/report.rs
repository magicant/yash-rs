// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! Error reporting utilities for built-ins
//!
//! This module provides utilities for printing error messages and computing
//! appropriate results (exit status and divert values) for built-ins.

use std::ops::ControlFlow::{Break, Continue};
use yash_env::Env;
#[cfg(doc)]
use yash_env::SharedSystem;
use yash_env::semantics::{Divert, ExitStatus};
#[cfg(doc)]
use yash_env::stack::Stack;
use yash_syntax::source::Location;
use yash_syntax::source::pretty::{Report, ReportType, Snippet, Span, SpanRole, add_span};

/// Convenience function for constructing an error report and a divert value.
///
/// If the environment is currently executing a built-in
/// ([`Stack::current_builtin`]), an annotation indicating the built-in name is
/// appended to the given report. The report is then converted into a string
/// using [`yash_env::io::report_to_string`] and returned with an appropriate
/// divert value.
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
pub fn prepare_report_message_and_divert<'e: 'r, 'r>(
    env: &'e Env,
    mut report: Report<'r>,
) -> (String, yash_env::semantics::Result) {
    let is_special_builtin;

    if let Some(builtin) = env.stack.current_builtin() {
        // Add an annotation indicating the built-in name
        let span = Span {
            range: builtin.name.origin.byte_range(),
            role: SpanRole::Supplementary {
                label: format!("while executing the {} built-in", builtin.name.value).into(),
            },
        };
        add_span(&builtin.name.origin.code, span, &mut report.snippets);

        let source = &builtin.name.origin.code.source;
        source.extend_with_context(&mut report.snippets);

        is_special_builtin = builtin.is_special;
    } else {
        is_special_builtin = false;
    }

    let text = yash_env::io::report_to_string(env, &report);
    let divert = if is_special_builtin {
        Break(Divert::Interrupt(None))
    } else {
        Continue(())
    };
    (text, divert)
}

/// Reports a message with the given exit status.
///
/// This is a convenience function for reporting a message with a specific exit
/// status. The message is converted to a string and [`Divert`] using
/// [`prepare_report_message_and_divert`], and then printed to the standard
/// error. The returned result contains the given exit status and the divert
/// value.
///
/// When the exit status is [`ExitStatus::FAILURE`] or [`ExitStatus::ERROR`],
/// you can use [`report_failure`] or [`report_error`] instead of this function,
/// respectively.
///
/// This function requires a mutable borrow of the environment to print the
/// message, so it is only usable when the `report` argument does not contain
/// any borrows from the environment. Otherwise, directly call
/// [`prepare_report_message_and_divert`], which only requires an immutable
/// borrow of the environment, to construct the message and divert value, and
/// then print the message yourself:
///
/// ```
/// # use futures_util::future::FutureExt as _;
/// # use yash_builtin::common::report::prepare_report_message_and_divert;
/// # use yash_env::builtin::Result;
/// # use yash_env::semantics::ExitStatus;
/// # use yash_syntax::source::pretty::{Report, ReportType, Snippet};
/// # use yash_syntax::syntax::Fd;
/// # async {
/// # let mut env = yash_env::Env::new_virtual();
/// # let mut report = Report::new();
/// # report.r#type = ReportType::Error;
/// # report.title = "cannot assign to read-only variable".into();
/// let (message, divert) = prepare_report_message_and_divert(&env, report);
/// env.system.print_error(&message).await;
/// Result::with_exit_status_and_divert(ExitStatus::FAILURE, divert)
/// # }.now_or_never().unwrap();
/// ```
#[inline]
pub async fn report<'a, R>(
    env: &mut Env,
    report: R,
    exit_status: ExitStatus,
) -> yash_env::builtin::Result
where
    R: Into<Report<'a>> + 'a,
{
    async fn inner(
        env: &mut Env,
        report: Report<'_>,
        exit_status: ExitStatus,
    ) -> yash_env::builtin::Result {
        let (message, divert) = prepare_report_message_and_divert(env, report);
        env.system.print_error(&message).await;
        yash_env::builtin::Result::with_exit_status_and_divert(exit_status, divert)
    }
    inner(env, report.into(), exit_status).await
}

/// Prints a failure message.
///
/// This is a simple shortcut for calling [`report`] with [`ExitStatus::FAILURE`].
#[inline]
pub async fn report_failure<'a, R>(env: &mut Env, report: R) -> yash_env::builtin::Result
where
    R: Into<Report<'a>> + 'a,
{
    self::report(env, report, ExitStatus::FAILURE).await
}

/// Prints an error message.
///
/// This is a simple shortcut for calling [`report`] with [`ExitStatus::ERROR`].
#[inline]
pub async fn report_error<'a, R>(env: &mut Env, report: R) -> yash_env::builtin::Result
where
    R: Into<Report<'a>> + 'a,
{
    self::report(env, report, ExitStatus::ERROR).await
}

/// Reports a simple message with the given exit status.
///
/// This function constructs a [`Report`] with the given title and prints it
/// using [`report`]. The message has no annotations except for the built-in
/// name which is added by [`prepare_report_message_and_divert`].
///
/// When the exit status is [`ExitStatus::FAILURE`] or [`ExitStatus::ERROR`],
/// you can use [`report_simple_failure`] or [`report_simple_error`] instead of
/// this function, respectively.
pub async fn report_simple(
    env: &mut Env,
    title: &str,
    exit_status: ExitStatus,
) -> yash_env::builtin::Result {
    let mut report = Report::new();
    report.r#type = ReportType::Error;
    report.title = title.into();
    self::report(env, report, exit_status).await
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
/// This function constructs a [`Report`] with a predefined title and a [`Span`]
/// created from the given label and location, and calls [`report_error`].
pub async fn syntax_error(
    env: &mut Env,
    label: &str,
    location: &Location,
) -> yash_env::builtin::Result {
    let mut report = Report::new();
    report.r#type = ReportType::Error;
    report.title = "command argument syntax error".into();
    report.snippets = Snippet::with_primary_span(location, label.into());
    report_error(env, report).await
}

/// Merges multiple reports into a single report.
///
/// If the given iterator is empty, this function returns `None`. Otherwise,
/// the first report's title and type are used as the merged report's title and
/// type. Snippets and footnotes from all reports are concatenated.
#[must_use]
pub fn merge_reports<'a, I, R>(reports: I) -> Option<Report<'a>>
where
    I: IntoIterator<Item = R> + 'a,
    R: Into<Report<'a>> + 'a,
{
    let mut reports = reports.into_iter();
    let mut first = reports.next()?.into();
    for report in reports {
        let report = report.into();
        first.snippets.extend(report.snippets);
        first.footnotes.extend(report.footnotes);
    }
    Some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::semantics::Field;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;

    #[test]
    fn divert_without_builtin() {
        let env = Env::new_virtual();
        let (_message, divert) = prepare_report_message_and_divert(&env, Report::new());
        assert_eq!(divert, Continue(()));
    }

    #[test]
    fn divert_with_special_builtin() {
        let mut env = Env::new_virtual();
        let env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("builtin"),
            is_special: true,
        }));

        let (_message, divert) = prepare_report_message_and_divert(&env, Report::new());
        assert_eq!(divert, Break(Divert::Interrupt(None)));
    }

    #[test]
    fn divert_with_non_special_builtin() {
        let mut env = Env::new_virtual();
        let env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("builtin"),
            is_special: false,
        }));

        let (_message, divert) = prepare_report_message_and_divert(&env, Report::new());
        assert_eq!(divert, Continue(()));
    }
}
