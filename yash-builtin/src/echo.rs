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

//! Echo built-in
//!
//! This module implements the [`echo` built-in], which prints its arguments.
//!
//! Which options the built-in accepts and whether it interprets escape
//! sequences depend on the [`Style`] selected by the [`ECHO_STYLE`] variable.
//!
//! [`echo` built-in]: https://magicant.github.io/yash-rs/builtins/echo.html

use crate::Result;
use crate::common::help::{BuiltinHelp, Help};
use crate::common::output;
use crate::common::report::report_error;
use crate::common::syntax::OptionSpec;
use std::borrow::Cow;
use std::ops::ControlFlow::{self, Break, Continue};
use yash_env::Env;
use yash_env::option::Option::Portable;
use yash_env::option::State::On;
use yash_env::semantics::Field;
use yash_env::source::pretty::{Footnote, FootnoteType, Report, ReportType, Snippet};
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;

/// Name of the variable that selects the [`Style`]
pub const ECHO_STYLE: &str = "ECHO_STYLE";

/// Behavior of the built-in, selected by the [`ECHO_STYLE`] variable
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Style {
    /// No options; escape sequences are always interpreted.
    ///
    /// This is the behavior POSIX requires on XSI-conformant systems.
    #[default]
    Sysv,
    /// `-n` is accepted; escape sequences are never interpreted.
    Bsd,
    /// `-n`, `-e`, and `-E` are accepted; escape sequences are interpreted
    /// only with `-e`.
    Gnu,
    /// `-n`, `-e`, and `-E` are accepted; escape sequences are interpreted
    /// unless `-E` is given.
    Zsh,
    /// `-n` is accepted; escape sequences are always interpreted.
    Dash,
    /// No options; escape sequences are never interpreted.
    Raw,
}

impl Style {
    /// Parses the value of the [`ECHO_STYLE`] variable.
    ///
    /// As in yash 2, only the first character of the value is significant, and
    /// it is case-insensitive. An empty or unknown value yields the default
    /// style.
    #[must_use]
    pub fn from_value(value: &str) -> Self {
        match value.chars().next().map(|c| c.to_ascii_uppercase()) {
            Some('B') => Self::Bsd,
            Some('G') => Self::Gnu,
            Some('Z') => Self::Zsh,
            Some('D') => Self::Dash,
            Some('R') => Self::Raw,
            _ => Self::Sysv,
        }
    }

    /// Returns the option characters accepted in this style.
    #[must_use]
    fn option_chars(self) -> &'static str {
        match self {
            Self::Sysv | Self::Raw => "",
            Self::Bsd | Self::Dash => "n",
            Self::Gnu | Self::Zsh => "neE",
        }
    }

    /// Returns whether escape sequences are interpreted when neither `-e` nor
    /// `-E` is given.
    #[must_use]
    fn escapes_by_default(self) -> bool {
        match self {
            Self::Sysv | Self::Zsh | Self::Dash => true,
            Self::Bsd | Self::Gnu | Self::Raw => false,
        }
    }
}

/// Parts of the help of the `echo` built-in that have the standard format
static STANDARD_HELP: BuiltinHelp = BuiltinHelp {
    summary: "print arguments",
    usage: &["echo [-n] [-e|-E] [STRING...]"],
    // The built-in parses its arguments without these specs because the
    // accepted options depend on the style, so they are used only for the help.
    options: &[
        OptionSpec::new()
            .short('n')
            .extension(true)
            .description("do not print the trailing newline (BSD, GNU, ZSH, DASH styles)"),
        OptionSpec::new()
            .short('e')
            .extension(true)
            .description("interpret escape sequences (GNU, ZSH styles)"),
        OptionSpec::new()
            .short('E')
            .extension(true)
            .description("do not interpret escape sequences (GNU, ZSH styles)"),
    ],
    manual_page_name: "echo",
};

/// Section of the help that explains the echo style
const ECHO_STYLE_HELP: &str = "\
Echo style:
  The ECHO_STYLE variable selects the style, which determines the accepted
  options and whether escape sequences are interpreted by default. The
  default style, SYSV, accepts no options and always interprets escape
  sequences.

";

/// Type of [`HELP`]
struct EchoHelp;

impl Help for EchoHelp {
    fn summary(&self) -> Cow<'_, str> {
        STANDARD_HELP.summary()
    }

    fn message(&self) -> Cow<'_, str> {
        let mut message = String::new();
        STANDARD_HELP.append_usage_and_options(&mut message);
        message.push_str(ECHO_STYLE_HELP);
        STANDARD_HELP.append_manual_link(&mut message);
        Cow::Owned(message)
    }
}

/// Help information of the `echo` built-in
///
/// This built-in does not accept the `--help` option, so the help is available
/// only from the `help` built-in. In addition to the usage and options of the
/// built-in, the message explains the [`ECHO_STYLE`] variable.
pub const HELP: &dyn Help = &EchoHelp;

/// Tests whether the argument is an option word made only of `option_chars`.
fn is_option(arg: &Field, option_chars: &str) -> bool {
    arg.value.strip_prefix('-').is_some_and(|letters| {
        !letters.is_empty() && letters.chars().all(|c| option_chars.contains(c))
    })
}

/// Settings derived from the options
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Settings {
    newline: bool,
    escapes: bool,
}

/// Parses the leading option words and returns the settings and the operands.
fn parse_options(style: Style, args: &[Field]) -> (Settings, &[Field]) {
    let option_chars = style.option_chars();
    let count = args
        .iter()
        .take_while(|arg| is_option(arg, option_chars))
        .count();
    let (options, operands) = args.split_at(count);

    let mut settings = Settings {
        newline: true,
        escapes: style.escapes_by_default(),
    };
    for c in options.iter().flat_map(|option| option.value[1..].chars()) {
        match c {
            'n' => settings.newline = false,
            'e' => settings.escapes = true,
            'E' => settings.escapes = false,
            _ => unreachable!("unexpected option character {c:?}"),
        }
    }
    (settings, operands)
}

/// Appends the operand to `out`, interpreting escape sequences.
///
/// Returns `Break` if the operand contains `\c`, after which nothing more
/// should be printed.
fn append_escaped(out: &mut Vec<u8>, operand: &str) -> ControlFlow<()> {
    // Operating on bytes is safe because no byte of a multibyte UTF-8
    // character equals an ASCII character.
    let mut bytes = operand.bytes().peekable();
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        match bytes.next() {
            Some(b'a') => out.push(0x07),
            Some(b'b') => out.push(0x08),
            Some(b'c') => return Break(()),
            Some(b'e') => out.push(0x1B),
            Some(b'f') => out.push(0x0C),
            Some(b'n') => out.push(b'\n'),
            Some(b'r') => out.push(b'\r'),
            Some(b't') => out.push(b'\t'),
            Some(b'v') => out.push(0x0B),
            Some(b'\\') => out.push(b'\\'),
            Some(b'0') => {
                // XSI specifies an 8-bit value, so excess bits are discarded.
                let value = (0..3)
                    .map_while(|_| bytes.next_if(|b| matches!(b, b'0'..=b'7')))
                    .fold(0u8, |value, digit| {
                        value.wrapping_mul(8).wrapping_add(digit - b'0')
                    });
                out.push(value);
            }
            Some(other) => out.extend([b'\\', other]),
            None => out.push(b'\\'),
        }
    }
    Continue(())
}

/// Composes the output of the built-in.
fn compose(settings: Settings, operands: &[Field]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, operand) in operands.iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        if !settings.escapes {
            out.extend_from_slice(operand.value.as_bytes());
        } else if append_escaped(&mut out, &operand.value).is_break() {
            return out;
        }
    }
    if settings.newline {
        out.push(b'\n');
    }
    out
}

/// Finds an argument whose result POSIX leaves implementation-defined.
///
/// Returns the argument and a label explaining why it is not portable.
fn find_non_portable(args: &[Field]) -> Option<(&Field, &'static str)> {
    let option_like = args.first().filter(|arg| is_option(arg, "neE")).map(|arg| {
        (
            arg,
            "the result of a first operand like this is implementation-defined",
        )
    });
    option_like.or_else(|| {
        args.iter().find(|arg| arg.value.contains('\\')).map(|arg| {
            (
                arg,
                "the result of an operand containing a backslash is implementation-defined",
            )
        })
    })
}

/// Creates a report of an argument whose result is implementation-defined.
///
/// The report points at `arg` with `label` and suggests the `printf` utility.
fn non_portable_report<'a>(arg: &'a Field, label: &'static str) -> Report<'a> {
    let mut report = Report::new();
    report.r#type = ReportType::Error;
    report.title = "non-portable argument".into();
    report.snippets = Snippet::with_primary_span(&arg.origin, label.into());
    report.footnotes.push(Footnote {
        r#type: FootnoteType::Note,
        label: "this error is reported because the `portable` shell option is enabled".into(),
    });
    report.footnotes.push(Footnote {
        r#type: FootnoteType::Suggestion,
        label: "use the `printf` utility for portable output".into(),
    });
    report
}

/// Executes the `echo` built-in.
///
/// This is the main entry point for the `echo` built-in.
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> Result
where
    S: Isatty + WriteAll,
{
    if env.options.get(Portable) == On
        && let Some((arg, label)) = find_non_portable(&args)
    {
        return report_error(env, non_portable_report(arg, label)).await;
    }

    let style = env
        .variables
        .get_scalar(ECHO_STYLE)
        .map(Style::from_value)
        .unwrap_or_default();
    let (settings, operands) = parse_options(style, &args);
    let out = compose(settings, operands);
    output(env, out).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::Concurrent;
    use yash_env::test_helper::{assert_stderr, assert_stdout};
    use yash_env::variable::Scope;

    #[test]
    fn style_from_value_accepts_known_names() {
        assert_eq!(Style::from_value("SYSV"), Style::Sysv);
        assert_eq!(Style::from_value("XSI"), Style::Sysv);
        assert_eq!(Style::from_value("BSD"), Style::Bsd);
        assert_eq!(Style::from_value("GNU"), Style::Gnu);
        assert_eq!(Style::from_value("ZSH"), Style::Zsh);
        assert_eq!(Style::from_value("DASH"), Style::Dash);
        assert_eq!(Style::from_value("RAW"), Style::Raw);
    }

    #[test]
    fn style_from_value_looks_only_at_first_character_case_insensitively() {
        assert_eq!(Style::from_value("bogus"), Style::Bsd);
        assert_eq!(Style::from_value("g"), Style::Gnu);
    }

    #[test]
    fn style_from_value_defaults_to_sysv() {
        assert_eq!(Style::from_value(""), Style::Sysv);
        assert_eq!(Style::from_value("unknown"), Style::Sysv);
    }

    #[test]
    fn help_message_explains_echo_style() {
        assert_eq!(
            HELP.message(),
            "\
Usage:
  echo [-n] [-e|-E] [STRING...]

Options:
  -n  do not print the trailing newline (BSD, GNU, ZSH, DASH styles)
  -e  interpret escape sequences (GNU, ZSH styles)
  -E  do not interpret escape sequences (GNU, ZSH styles)

Echo style:
  The ECHO_STYLE variable selects the style, which determines the accepted
  options and whether escape sequences are interpreted by default. The
  default style, SYSV, accepts no options and always interprets escape
  sequences.

See <https://magicant.github.io/yash-rs/builtins/echo.html> for details.
"
        );
    }
    #[test]
    fn parse_options_without_options() {
        let args = Field::dummies(["-n", "foo"]);

        let (settings, operands) = parse_options(Style::Sysv, &args);

        assert_eq!(
            settings,
            Settings {
                newline: true,
                escapes: true
            }
        );
        assert_eq!(operands, args);
    }

    #[test]
    fn parse_options_consumes_leading_options() {
        let args = Field::dummies(["-n", "-n", "foo", "-n"]);

        let (settings, operands) = parse_options(Style::Bsd, &args);

        assert_eq!(
            settings,
            Settings {
                newline: false,
                escapes: false
            }
        );
        assert_eq!(operands, &args[2..]);
    }

    #[test]
    fn parse_options_applies_later_letter_in_same_word() {
        let args = Field::dummies(["-eE", "foo"]);

        let (settings, _) = parse_options(Style::Gnu, &args);

        assert!(!settings.escapes);
    }

    #[test]
    fn parse_options_applies_later_word() {
        let args = Field::dummies(["-E", "-ne", "foo"]);

        let (settings, _) = parse_options(Style::Zsh, &args);

        assert_eq!(
            settings,
            Settings {
                newline: false,
                escapes: true
            }
        );
    }

    #[test]
    fn parse_options_stops_at_non_option() {
        for arg in ["-", "--", "-x", "-nx", "foo"] {
            let args = Field::dummies([arg, "-n"]);

            let (settings, operands) = parse_options(Style::Gnu, &args);

            assert!(settings.newline, "{arg:?}");
            assert_eq!(operands, args, "{arg:?}");
        }
    }

    #[test]
    fn parse_options_rejects_letters_not_in_style() {
        let args = Field::dummies(["-e", "foo"]);

        let (settings, operands) = parse_options(Style::Dash, &args);

        assert!(settings.escapes);
        assert_eq!(operands, args);
    }

    #[test]
    fn append_escaped_plain_text() {
        let mut out = Vec::new();

        let result = append_escaped(&mut out, "foo 日本");

        assert_eq!(result, Continue(()));
        assert_eq!(out, "foo 日本".as_bytes());
    }

    #[test]
    fn append_escaped_character_escapes() {
        let mut out = Vec::new();

        let result = append_escaped(&mut out, r"\a\b\e\f\n\r\t\v\\");

        assert_eq!(result, Continue(()));
        assert_eq!(out, b"\x07\x08\x1B\x0C\n\r\t\x0B\\");
    }

    #[test]
    fn append_escaped_stops_at_backslash_c() {
        let mut out = Vec::new();

        let result = append_escaped(&mut out, r"foo\cbar");

        assert_eq!(result, Break(()));
        assert_eq!(out, b"foo");
    }

    #[test]
    fn append_escaped_octal_escapes() {
        let mut out = Vec::new();

        let result = append_escaped(&mut out, r"\0\01\012\0101\08");

        assert_eq!(result, Continue(()));
        assert_eq!(out, b"\x00\x01\x0AA\x008");
    }

    #[test]
    fn append_escaped_octal_escape_truncated_to_8_bits() {
        let mut out = Vec::new();

        let result = append_escaped(&mut out, r"\0777\0377");

        assert_eq!(result, Continue(()));
        assert_eq!(out, b"\xFF\xFF");
    }

    #[test]
    fn append_escaped_keeps_unknown_escape_and_trailing_backslash() {
        let mut out = Vec::new();

        let result = append_escaped(&mut out, r"\q\1\");

        assert_eq!(result, Continue(()));
        assert_eq!(out, br"\q\1\");
    }

    #[test]
    fn compose_joins_operands_with_spaces() {
        let settings = Settings {
            newline: true,
            escapes: false,
        };
        let operands = Field::dummies(["foo", r"b\ar", ""]);

        let out = compose(settings, &operands);

        assert_eq!(out, b"foo b\\ar \n");
    }

    #[test]
    fn compose_without_operands() {
        let settings = Settings {
            newline: true,
            escapes: true,
        };

        let out = compose(settings, &[]);

        assert_eq!(out, b"\n");
    }

    #[test]
    fn compose_without_newline() {
        let settings = Settings {
            newline: false,
            escapes: true,
        };
        let operands = Field::dummies(["foo"]);

        let out = compose(settings, &operands);

        assert_eq!(out, b"foo");
    }

    #[test]
    fn compose_stops_at_backslash_c() {
        let settings = Settings {
            newline: true,
            escapes: true,
        };
        let operands = Field::dummies(["foo", r"b\car", "baz"]);

        let out = compose(settings, &operands);

        assert_eq!(out, b"foo b");
    }

    #[test]
    fn find_non_portable_accepts_portable_arguments() {
        let args = Field::dummies(["foo", "-", "--", "-x", "-n"]);

        assert_eq!(find_non_portable(&args), None);
    }

    #[test]
    fn find_non_portable_detects_option_like_first_operand() {
        let args = Field::dummies(["-Ene", "foo"]);

        let (arg, _) = find_non_portable(&args).unwrap();

        assert_eq!(arg.value, "-Ene");
    }

    #[test]
    fn find_non_portable_detects_backslash() {
        let args = Field::dummies(["foo", r"a\b", r"c\d"]);

        let (arg, _) = find_non_portable(&args).unwrap();

        assert_eq!(arg.value, r"a\b");
    }

    #[test]
    fn main_prints_operands_in_default_style() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        let args = Field::dummies(["-n", r"a\tb"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result, Result::default());
        assert_stdout(&state, |stdout| assert_eq!(stdout, "-n a\tb\n"));
    }

    #[test]
    fn main_applies_echo_style() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.variables
            .get_or_new(ECHO_STYLE, Scope::Global)
            .assign("BSD", None)
            .unwrap();
        let args = Field::dummies(["-n", r"a\tb"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result, Result::default());
        assert_stdout(&state, |stdout| assert_eq!(stdout, r"a\tb"));
    }

    #[test]
    fn main_rejects_non_portable_argument_under_portable_option() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Portable, On);
        let args = Field::dummies(["foo", r"a\tb"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result.exit_status(), ExitStatus::ERROR);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("non-portable argument"), "{stderr:?}");
            assert!(stderr.contains(r"a\tb"), "{stderr:?}");
        });
    }

    #[test]
    fn main_prints_portable_arguments_under_portable_option() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Portable, On);
        let args = Field::dummies(["foo", "--"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result, Result::default());
        assert_stdout(&state, |stdout| assert_eq!(stdout, "foo --\n"));
    }
}
