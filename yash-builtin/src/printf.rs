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

//! Printf built-in
//!
//! This module implements the [`printf` built-in], which formats its arguments
//! according to a format string.
//!
//! [`printf` built-in]: https://magicant.github.io/yash-rs/builtins/printf.html

use crate::Result;
use crate::common::help::{
    BuiltinHelp, HELP_OPTION, Help, OptionSpecsWithHelp, WithHelp, parse_arguments_with_help,
    print_help_of_current_builtin,
};
use crate::common::output;
use crate::common::report::{
    group_errors_by_kind, merge_reports, report_error, report_failure, report_simple_error,
};
use crate::common::syntax::{Mode, OptionSpec};
use crate::typeset::AssignReadOnlyError;
use format::{Extension, FormatError};
use yash_env::Env;
use yash_env::option::Option::Portable;
use yash_env::option::State::On;
use yash_env::semantics::Field;
use yash_env::source::pretty::{Footnote, FootnoteType, Report, ReportType, Snippet};
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;
use yash_env::variable::Scope;

mod apply;
mod arg;
mod format;
mod number;

const OPTION_SPECS: OptionSpecsWithHelp = OptionSpecsWithHelp::new(&[
    OptionSpec::new()
        .short('v')
        .long("variable")
        .argument("VARIABLE")
        .extension(true)
        .description("assign the result to VARIABLE instead of printing it"),
    HELP_OPTION,
]);

/// Help information of the `printf` built-in
pub const HELP: &dyn Help = &BuiltinHelp {
    summary: "print formatted text",
    usage: &["printf [-v VARIABLE] FORMAT [ARGUMENT...]"],
    options: OPTION_SPECS.get(),
    manual_page_name: "printf",
};

/// Creates a report of an error in the format operand.
fn format_error_report<'a>(format: &'a Field, error: &FormatError) -> Report<'a> {
    let mut report = Report::new();
    report.r#type = ReportType::Error;
    report.title = "invalid format".into();
    report.snippets = Snippet::with_primary_span(&format.origin, error.to_string().into());
    report
}

/// Creates a report of a feature of the format that POSIX does not specify.
fn non_portable_format_report(format: &Field, extension: Extension) -> Report<'_> {
    let label = match extension {
        Extension::Escape(None) => "a trailing backslash may not work in other shells".to_owned(),
        Extension::Escape(Some(c)) => {
            format!("the escape sequence `\\{c}` may not work in other shells")
        }
        Extension::Asterisk => {
            "`*` for a field width or precision may not work in other shells".to_owned()
        }
        Extension::Conversion(c) => {
            format!("the conversion specifier `{c}` may not work in other shells")
        }
        Extension::MixedNumbering => {
            "mixing numbered and unnumbered conversions may not work in other shells".to_owned()
        }
        Extension::CharPrecision => {
            "a precision for the conversion specifier `c` may not work in other shells".to_owned()
        }
        Extension::LargeOctal => {
            "an octal escape sequence whose value exceeds a byte may not work in other shells"
                .to_owned()
        }
    };
    let mut report = Report::new();
    report.r#type = ReportType::Error;
    report.title = "non-portable format".into();
    report.snippets = Snippet::with_primary_span(&format.origin, label.into());
    report.footnotes.push(Footnote {
        r#type: FootnoteType::Note,
        label: "this error is reported because the `portable` shell option is enabled".into(),
    });
    report
}

/// Creates a report of an invalid variable name given to the `-v` option.
fn invalid_variable_name_report(name: &Field) -> Report<'_> {
    let mut report = Report::new();
    report.r#type = ReportType::Error;
    report.title = "cannot assign to variable".into();
    report.snippets = Snippet::with_primary_span(
        &name.origin,
        format!("variable name {:?} is not valid", name.value).into(),
    );
    report
}

/// Assigns the result to the variable.
async fn assign<S>(env: &mut Env<S>, name: Field, value: Vec<u8>) -> Result
where
    S: Isatty + WriteAll,
{
    // As in command substitution, invalid UTF-8 sequences are replaced.
    let value = String::from_utf8(value)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    let mut variable = env.get_or_create_variable(name.value.clone(), Scope::Global);
    match variable.assign(value, name.origin) {
        Ok(_) => Result::default(),
        Err(e) => {
            let error = AssignReadOnlyError {
                name: name.value,
                new_value: e.new_value,
                assigned_location: e.assigned_location.unwrap(),
                read_only_location: e.read_only_location,
            };
            report_failure(env, &error).await
        }
    }
}

/// Executes the `printf` built-in.
///
/// This is the main entry point for the `printf` built-in.
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> Result
where
    S: Isatty + WriteAll,
{
    let mode = Mode::with_env(env);
    let (options, operands) = match parse_arguments_with_help(OPTION_SPECS, mode, args) {
        Ok(WithHelp::Main(parsed)) => parsed,
        Ok(WithHelp::Help) => return print_help_of_current_builtin(env).await,
        Err(e) => return report_error(env, &e).await,
    };

    let variable = options
        .into_iter()
        .next_back()
        .and_then(|option| option.argument);
    if let Some(name) = &variable
        && (name.value.is_empty() || name.value.contains('='))
    {
        return report_error(env, invalid_variable_name_report(name)).await;
    }

    let Some((format_field, args)) = operands.split_first() else {
        return report_simple_error(env, "the format operand is missing").await;
    };
    let format = match format::parse(&format_field.value) {
        Ok(format) => format,
        Err(e) => return report_error(env, format_error_report(format_field, &e)).await,
    };

    let portable = env.options.get(Portable) == On;
    if portable && let Some(extension) = format.extension {
        return report_error(env, non_portable_format_report(format_field, extension)).await;
    }

    let (out, errors) = apply::apply(&format, format_field, args, portable);

    let non_portable = errors
        .iter()
        .filter(|e| matches!(e, apply::Error::NonPortable { .. }));
    if let Some(report) = merge_reports(non_portable) {
        return report_error(env, report).await;
    }

    let mut result = match variable {
        None => output(env, &out).await,
        Some(name) => assign(env, name, out).await,
    };
    for (_, group) in group_errors_by_kind(&errors) {
        if let Some(report) = merge_reports(group) {
            result = result.max(report_failure(env, report).await);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::semantics::ExitStatus;
    use yash_env::source::Location;
    use yash_env::system::Concurrent;
    use yash_env::test_helper::{assert_stderr, assert_stdout};
    use yash_env::variable::Value;

    fn env_with_state() -> (Env<Rc<Concurrent<VirtualSystem>>>, VirtualSystem) {
        let system = VirtualSystem::new();
        let env = Env::with_system(Rc::new(Concurrent::new(system.clone())));
        (env, system)
    }

    #[test]
    fn main_prints_formatted_arguments() {
        let (mut env, system) = env_with_state();
        let args = Field::dummies(["%s=%d\\n", "a", "1", "b", "2"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result, Result::default());
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, "a=1\nb=2\n"));
    }

    #[test]
    fn main_accepts_double_hyphen() {
        let (mut env, system) = env_with_state();
        let args = Field::dummies(["--", "-%s", "x"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result, Result::default());
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, "-x"));
    }

    #[test]
    fn main_assigns_to_variable() {
        let (mut env, system) = env_with_state();
        let args = Field::dummies(["-v", "var", "[%s]\\n", "foo"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result, Result::default());
        assert_eq!(
            env.variables.get("var").unwrap().value,
            Some(Value::scalar("[foo]\n"))
        );
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn main_replaces_invalid_utf8_in_assigned_value() {
        let (mut env, _system) = env_with_state();
        let args = Field::dummies(["-v", "var", "a\\377b"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result, Result::default());
        assert_eq!(
            env.variables.get("var").unwrap().value,
            Some(Value::scalar("a\u{FFFD}b"))
        );
    }

    #[test]
    fn main_rejects_read_only_variable() {
        let (mut env, system) = env_with_state();
        let mut var = env.get_or_create_variable("var", Scope::Global);
        var.assign("old", None).unwrap();
        var.make_read_only(Location::dummy("read-only"));
        let args = Field::dummies(["-v", "var", "new"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result.exit_status(), ExitStatus::FAILURE);
        assert_eq!(
            env.variables.get("var").unwrap().value,
            Some(Value::scalar("old"))
        );
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("read-only"), "{stderr:?}")
        });
    }

    #[test]
    fn main_rejects_invalid_variable_name() {
        let (mut env, system) = env_with_state();
        let args = Field::dummies(["-v", "a=b", "x"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result.exit_status(), ExitStatus::ERROR);
        assert_eq!(env.variables.get("a=b"), None);
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("cannot assign to variable"), "{stderr:?}")
        });
    }

    #[test]
    fn main_rejects_missing_format() {
        let (mut env, system) = env_with_state();

        let result = main(&mut env, vec![]).now_or_never().unwrap();

        assert_eq!(result.exit_status(), ExitStatus::ERROR);
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("format operand is missing"), "{stderr:?}")
        });
    }

    #[test]
    fn main_rejects_invalid_format_without_printing() {
        let (mut env, system) = env_with_state();
        let args = Field::dummies(["foo%y"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result.exit_status(), ExitStatus::ERROR);
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("invalid format"), "{stderr:?}")
        });
    }

    #[test]
    fn main_prints_partial_result_for_invalid_argument() {
        let (mut env, system) = env_with_state();
        let args = Field::dummies(["%d|%d\\n", "1x", "2"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result.exit_status(), ExitStatus::FAILURE);
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, "1|2\n"));
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("invalid integer"), "{stderr:?}")
        });
    }

    #[test]
    fn main_rejects_non_portable_format_under_portable_option() {
        let (mut env, system) = env_with_state();
        env.options.set(Portable, On);
        let args = Field::dummies(["%s %f", "a", "1"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result.exit_status(), ExitStatus::ERROR);
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("non-portable format"), "{stderr:?}")
        });
    }

    #[test]
    fn main_rejects_non_portable_argument_under_portable_option() {
        let (mut env, system) = env_with_state();
        env.options.set(Portable, On);
        let args = Field::dummies(["%b", "a\\eb"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result.exit_status(), ExitStatus::ERROR);
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("non-portable argument"), "{stderr:?}")
        });
    }

    #[test]
    fn main_prints_portable_format_under_portable_option() {
        let (mut env, system) = env_with_state();
        env.options.set(Portable, On);
        let args = Field::dummies(["%5.2s|%b\\n", "abc", "\\0101"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result, Result::default());
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, "   ab|A\n"));
    }
}
