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

//! Support for built-ins that POSIX specifies with no options and no operands
//!
//! POSIX describes the [`true`](crate::r#true) and [`false`](crate::r#false)
//! built-ins with both the OPTIONS and OPERANDS sections listed as "None.",
//! which XCU 1.4 Utility Description Defaults defines as meaning the
//! implementation need not support any options or operands. An argument given
//! to such a built-in therefore has no portable meaning, so this module warns
//! about it while the `portable` shell option is on.

use super::report::prepare_report_message_and_divert;
use yash_env::Env;
use yash_env::option::Option::Portable;
use yash_env::option::State;
use yash_env::semantics::Field;
use yash_env::source::pretty::{Footnote, FootnoteType, Report, ReportType, Snippet};
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;

/// Warns that the built-in was given an argument, if it was and the `portable`
/// shell option is on.
///
/// The message is only a warning: it does not affect the exit status, and the
/// built-in still does what it would have done without the argument.
pub(crate) async fn warn_if_any_argument<S>(env: &mut Env<S>, args: &[Field])
where
    S: Isatty + WriteAll,
{
    let Some(first) = args.first() else { return };
    if env.options.get(Portable) != State::On {
        return;
    }

    let mut report = Report::new();
    report.r#type = ReportType::Warning;
    report.title = "non-portable argument".into();
    report.snippets = Snippet::with_primary_span(
        &first.origin,
        "POSIX does not require this built-in to accept any argument".into(),
    );
    report.footnotes.push(Footnote {
        r#type: FootnoteType::Note,
        label: "this warning is reported because the `portable` shell option is enabled".into(),
    });

    let (message, _divert) = prepare_report_message_and_divert(env, report);
    env.system.print_error(&message).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::option::State::{Off, On};
    use yash_env::system::Concurrent;
    use yash_env::test_helper::assert_stderr;

    #[test]
    fn no_warning_without_arguments() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Portable, On);

        warn_if_any_argument(&mut env, &[]).now_or_never().unwrap();

        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn no_warning_without_portable_option() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Portable, Off);
        let args = Field::dummies(["foo"]);

        warn_if_any_argument(&mut env, &args)
            .now_or_never()
            .unwrap();

        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn warning_with_portable_option() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Portable, On);
        let args = Field::dummies(["foo"]);

        warn_if_any_argument(&mut env, &args)
            .now_or_never()
            .unwrap();

        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("non-portable argument"), "{stderr:?}");
            assert!(stderr.contains("foo"), "{stderr:?}");
            assert!(stderr.contains("portable"), "{stderr:?}");
        });
    }

    #[test]
    fn warning_points_at_the_first_argument_only() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Portable, On);
        let args = Field::dummies(["--", "bar"]);

        warn_if_any_argument(&mut env, &args)
            .now_or_never()
            .unwrap();

        assert_stderr(&state, |stderr| {
            assert_eq!(
                stderr.matches("non-portable argument").count(),
                1,
                "{stderr:?}"
            );
            assert!(!stderr.contains("bar"), "{stderr:?}");
        });
    }
}
