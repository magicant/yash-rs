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
//!
//! The same table is used by [`parse_or_print_help`], which prints the help of
//! a built-in invoked with the `--help` option. See the
//! [`common::help`](crate::common::help) module for how to give a built-in
//! help.

use crate::common::help::{
    BuiltinHelp, HELP_OPTION, Help, OptionSpecsWithHelp, parse_or_print_help,
};
use crate::common::output;
use crate::common::report::{merge_reports, report_error, report_failure, report_simple_error};
use crate::common::syntax::Mode;
use std::collections::HashMap;
use std::ops::ControlFlow;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::source::pretty::{Footnote, FootnoteType, Report, ReportType, Snippet};
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;
use yash_quote::quoted;

const OPTION_SPECS: OptionSpecsWithHelp = OptionSpecsWithHelp::new(&[HELP_OPTION]);

/// Help information of the `help` built-in
pub static HELP: BuiltinHelp = BuiltinHelp {
    summary: "print the help of built-ins",
    usage: &["help [name...]"],
    options: OPTION_SPECS.get(),
    manual_page_name: "help",
};

/// Table of help information keyed by the name of the built-in
#[derive(Clone)]
struct HelpTable(HashMap<&'static str, &'static dyn Help>);

impl Default for HelpTable {
    fn default() -> Self {
        Self(HashMap::from([
            ("help", &HELP as &dyn Help),
            ("pwd", &crate::pwd::HELP),
        ]))
    }
}

fn table<S>(env: &mut Env<S>) -> &mut HashMap<&'static str, &'static dyn Help> {
    &mut env.any.get_or_insert_with(Box::<HelpTable>::default).0
}

/// Registers the help information of a built-in.
///
/// The `help` built-in prints the registered help for the built-in named
/// `name`, and so does the built-in itself if it accepts the `--help` option.
/// If help has already been registered for the name, it is replaced.
///
/// The help of the built-ins defined in this crate is registered by default,
/// so this function is needed only for built-ins defined elsewhere. Call it
/// when adding the built-in to [`Env::builtins`]. A `static` item of
/// [`BuiltinHelp`] can be passed as `help`.
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
    let mode = Mode::with_env(env);
    let operands = match parse_or_print_help(env, OPTION_SPECS, mode, args).await {
        ControlFlow::Break(result) => return result,
        ControlFlow::Continue(Ok((_, operands))) => operands,
        ControlFlow::Continue(Err(error)) => return report_error(env, &error).await,
    };
    if operands.is_empty() {
        // TODO: List the built-ins with their summaries
        return report_simple_error(env, "listing built-ins is not implemented yet").await;
    }
    print(env, &operands).await
}

/// Prints the help of the built-ins named by `operands`.
///
/// This function reports an error for each operand that has no help.
pub(crate) async fn print<S>(env: &mut Env<S>, operands: &[Field]) -> crate::Result
where
    S: Isatty + WriteAll,
{
    let table = table(env);
    let mut message = String::new();
    let mut reports = Vec::new();
    for operand in operands {
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
    use yash_env::stack::{Builtin, Frame};
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
    fn default_table_has_help_only_for_builtins_in_iter() {
        let names: Vec<&str> = crate::iter::<Rc<Concurrent<VirtualSystem>>>()
            .map(|(name, _)| name)
            .collect();
        for name in HelpTable::default().0.keys() {
            assert!(names.contains(name), "{name:?} should be in crate::iter");
        }
    }

    #[test]
    fn reports_unimplemented_listing_without_operand() {
        let (mut env, system) = env();

        let result = main(&mut env, vec![]).now_or_never().unwrap();

        assert_eq!(result, crate::Result::from(ExitStatus::ERROR));
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("not implemented"), "stderr = {stderr:?}");
        });
    }

    #[test]
    fn prints_help_of_operand() {
        let (mut env, system) = env();

        let result = main(&mut env, Field::dummies(["help"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::default());
        assert_stdout(&system.state, |stdout| {
            assert!(stdout.starts_with("help: print the help of built-ins\n\nUsage:\n"))
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
        register(&mut env, "help", &DummyHelp);

        let result = main(&mut env, Field::dummies(["help"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::default());
        assert_stdout(&system.state, |stdout| {
            assert_eq!(stdout, "help: dummy\n\ndummy help\n")
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
    fn prints_own_help_for_help_option() {
        let (mut env, system) = env();
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("help"),
            is_special: false,
        }));

        let result = main(&mut env, Field::dummies(["--help"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::default());
        assert_stdout(&system.state, |stdout| {
            assert!(
                stdout
                    .starts_with("help: print the help of built-ins\n\nUsage:\n  help [name...]\n"),
                "stdout = {stdout:?}"
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
