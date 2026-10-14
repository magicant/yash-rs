// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
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

//! Help built-in
//!
//! This module implements the `help` built-in, which prints the help message
//! of built-ins.
//!
//! The help information of built-ins is kept in a table stored in
//! [`Env::any`]. The table initially contains the help of the built-ins
//! defined in this crate. Built-ins defined outside this crate can be given
//! help with [`register`].

use crate::common::help::{HELP_OPTION, Help};
use crate::common::output;
use crate::common::report::{merge_reports, report_error, report_failure};
use crate::common::syntax::{Mode, OptionSpec, parse_arguments};
use std::collections::HashMap;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::source::pretty::{Footnote, FootnoteType, Report, ReportType, Snippet};
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;
use yash_quote::quoted;

const OPTION_SPECS: &[OptionSpec] = &[HELP_OPTION];

/// Table of help information keyed by the name of the built-in
#[derive(Clone)]
struct HelpTable(HashMap<&'static str, &'static dyn Help>);

impl Default for HelpTable {
    fn default() -> Self {
        Self(HashMap::from([("pwd", &crate::pwd::HELP as &dyn Help)]))
    }
}

fn table<S>(env: &mut Env<S>) -> &mut HashMap<&'static str, &'static dyn Help> {
    &mut env.any.get_or_insert_with(Box::<HelpTable>::default).0
}

/// Registers the help information of a built-in.
///
/// The `help` built-in prints the registered help for the built-in named
/// `name`. If help has already been registered for the name, it is replaced.
pub fn register<S>(env: &mut Env<S>, name: &'static str, help: &'static dyn Help) {
    table(env).insert(name, help);
}

/// Appends the help message of a built-in to `message`.
///
/// The message begins with a line consisting of `name` and the summary,
/// followed by an empty line and the [body](Help::message).
fn append_message(message: &mut String, name: &str, help: &dyn Help) {
    message.push_str(name);
    message.push_str(": ");
    message.push_str(&help.summary());
    message.push_str("\n\n");
    message.push_str(&help.message());
}

fn no_help_report(operand: &Field) -> Report<'_> {
    let mut report = Report::new();
    report.r#type = ReportType::Error;
    report.title = "no help available".into();
    report.snippets = Snippet::with_primary_span(
        &operand.origin,
        format!("{}: not a built-in or has no help", operand.value).into(),
    );
    report.footnotes.push(Footnote {
        r#type: FootnoteType::Suggestion,
        label: format!(
            "try `man -k {}` to search the manual pages",
            quoted(&operand.value)
        )
        .into(),
    });
    report
}

/// Entry point of the `help` built-in
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> crate::Result
where
    S: Isatty + WriteAll,
{
    let (options, operands) = match parse_arguments(OPTION_SPECS, Mode::with_env(env), args) {
        Ok(result) => result,
        Err(error) => return report_error(env, &error).await,
    };

    if !options.is_empty() {
        todo!("help --help");
    }
    if operands.is_empty() {
        todo!("help with no operand");
    }

    let table = table(env);
    let mut message = String::new();
    let mut reports = Vec::new();
    for operand in &operands {
        match table.get(operand.value.as_str()) {
            Some(&help) => {
                if !message.is_empty() {
                    message.push('\n');
                }
                append_message(&mut message, &operand.value, help);
            }
            None => reports.push(no_help_report(operand)),
        }
    }

    let output_result = output(env, &message).await;
    let error_result = match merge_reports(reports) {
        Some(report) => report_failure(env, report).await,
        None => crate::Result::default(),
    };
    output_result.max(error_result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::borrow::Cow;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::Concurrent;
    use yash_env::test_helper::{assert_stderr, assert_stdout};

    fn env() -> (Env<Rc<Concurrent<VirtualSystem>>>, VirtualSystem) {
        let system = VirtualSystem::new();
        let env = Env::with_system(Rc::new(Concurrent::new(system.clone())));
        (env, system)
    }

    struct DummyHelp;

    impl Help for DummyHelp {
        fn summary(&self) -> Cow<'_, str> {
            Cow::Borrowed("dummy")
        }

        fn message(&self) -> Cow<'_, str> {
            Cow::Borrowed("dummy help\n")
        }
    }

    #[test]
    fn prints_help_of_operand() {
        let (mut env, system) = env();

        let result = main(&mut env, Field::dummies(["pwd"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::default());
        assert_stdout(&system.state, |stdout| {
            assert!(stdout.starts_with("pwd: print the working directory path\n\nUsage:\n"))
        });
    }
    #[test]
    fn prints_registered_help() {
        let (mut env, system) = env();
        register(&mut env, "dummy", &DummyHelp);

        let result = main(&mut env, Field::dummies(["dummy"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::default());
        assert_stdout(&system.state, |stdout| {
            assert_eq!(stdout, "dummy: dummy\n\ndummy help\n")
        });
    }

    #[test]
    fn registered_help_replaces_existing_one() {
        let (mut env, system) = env();
        register(&mut env, "pwd", &DummyHelp);

        let result = main(&mut env, Field::dummies(["pwd"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::default());
        assert_stdout(&system.state, |stdout| {
            assert_eq!(stdout, "pwd: dummy\n\ndummy help\n")
        });
    }

    #[test]
    fn separates_messages_of_multiple_operands_with_empty_line() {
        let (mut env, system) = env();
        register(&mut env, "foo", &DummyHelp);
        register(&mut env, "bar", &DummyHelp);

        let result = main(&mut env, Field::dummies(["foo", "bar"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::default());
        assert_stdout(&system.state, |stdout| {
            assert_eq!(
                stdout,
                "foo: dummy\n\ndummy help\n\nbar: dummy\n\ndummy help\n"
            )
        });
    }

    #[test]
    fn reports_operand_without_help() {
        let (mut env, system) = env();

        let result = main(&mut env, Field::dummies(["foo"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::from(ExitStatus::FAILURE));
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("foo"), "stderr = {stderr:?}");
            assert!(stderr.contains("man -k foo"), "stderr = {stderr:?}");
        });
    }

    #[test]
    fn prints_help_of_other_operands_after_operand_without_help() {
        let (mut env, system) = env();
        register(&mut env, "foo", &DummyHelp);
        register(&mut env, "bar", &DummyHelp);

        let result = main(&mut env, Field::dummies(["foo", "baz", "bar"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::from(ExitStatus::FAILURE));
        assert_stdout(&system.state, |stdout| {
            assert_eq!(
                stdout,
                "foo: dummy\n\ndummy help\n\nbar: dummy\n\ndummy help\n"
            )
        });
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("man -k baz"), "stderr = {stderr:?}");
        });
    }
}
