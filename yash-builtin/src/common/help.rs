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

//! Help messages of built-ins
//!
//! This module provides [`BuiltinHelp`], an implementation of [`Help`] that
//! generates the help message from structured data, including the option
//! specs the built-in's parser uses.

use super::syntax::{OptionArgumentSpec, OptionSpec};
use std::borrow::Cow;
use std::fmt::Write as _;
use std::ops::Deref;

/// Base URL of the manual pages of the built-ins
const MANUAL_BASE_URL: &str = "https://magicant.github.io/yash-rs/builtins/";

/// Help information of a built-in
///
/// An implementation of this trait provides the help message of a built-in.
pub trait Help {
    /// Returns a one-line summary of the built-in.
    fn summary(&self) -> Cow<'_, str>;

    /// Returns the body of the help message of the built-in.
    ///
    /// The `help` built-in prints the body after a line consisting of the
    /// name of the built-in and the [summary](Self::summary), so the body
    /// should not repeat them. The body should end with a newline.
    fn message(&self) -> Cow<'_, str>;
}

/// Spec of the `--help` option
///
/// A built-in that accepts `--help` includes this spec at the end of its
/// [`OptionSpecsWithHelp`].
pub const HELP_OPTION: OptionSpec<'static> = OptionSpec::new()
    .long("help")
    .extension(true)
    .description("print this help");

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Const version of `*spec == HELP_OPTION`
const fn is_help_option(spec: &OptionSpec) -> bool {
    // HELP_OPTION has no short name and takes no argument.
    let Some(long) = spec.get_long() else {
        return false;
    };
    spec.get_short().is_none()
        && str_eq(long, "help")
        && matches!(spec.get_argument(), OptionArgumentSpec::None)
        && spec.is_extension() == HELP_OPTION.is_extension()
        && str_eq(spec.get_description(), HELP_OPTION.get_description())
}

/// Option specs of a built-in that accepts `--help`
///
/// This is a list of option specs whose last element is [`HELP_OPTION`].
/// [`new`](Self::new) ensures this condition, so a built-in defining the
/// option specs in a `const` item fails to compile if it forgets to include
/// `HELP_OPTION`.
///
/// A built-in that accepts `--help` defines its option specs as a `const` item
/// of this type and passes the result of [`get`](Self::get) to
/// [`BuiltinHelp::options`] so that its help lists the same options as the
/// built-in accepts. See [`BuiltinHelp`] for an example.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptionSpecsWithHelp<'a>(&'a [OptionSpec<'a>]);

impl<'a> OptionSpecsWithHelp<'a> {
    /// Creates a new instance.
    ///
    /// # Panics
    ///
    /// If the last element of `specs` is not [`HELP_OPTION`]. When evaluated
    /// in a const context, this is a compile-time error.
    #[must_use]
    pub const fn new(specs: &'a [OptionSpec<'a>]) -> Self {
        assert!(
            matches!(specs.last(), Some(last) if is_help_option(last)),
            "the last option spec should be HELP_OPTION"
        );
        Self(specs)
    }

    /// Returns the option specs, including [`HELP_OPTION`].
    ///
    /// This is a const version of [`deref`](Self::deref). Unlike `deref`, the
    /// returned slice is not bound to the lifetime of `self`.
    #[must_use]
    pub const fn get(&self) -> &'a [OptionSpec<'a>] {
        self.0
    }
}

impl<'a> Deref for OptionSpecsWithHelp<'a> {
    type Target = [OptionSpec<'a>];

    fn deref(&self) -> &[OptionSpec<'a>] {
        self.0
    }
}

/// Help information of a built-in
///
/// This is the standard implementation of [`Help`], which formats the help
/// message from structured data so that all built-ins have help in the same
/// format. The body of the help message consists of the usage lines, the
/// option list, and a link to the manual page. The usage lines and the option
/// list are omitted if empty. The option list is generated from the option
/// specs so that it stays in sync with the parser.
///
/// The link points to a page in the yash-rs documentation at
/// <https://magicant.github.io/yash-rs/builtins/>. For a built-in that is not
/// documented there, or to print the help in another format, implement
/// [`Help`] in your own type instead.
///
/// # Example
///
/// ```
/// use yash_builtin::common::help::{BuiltinHelp, HELP_OPTION, Help, OptionSpecsWithHelp};
/// use yash_builtin::common::syntax::OptionSpec;
///
/// const OPTION_SPECS: OptionSpecsWithHelp = OptionSpecsWithHelp::new(&[
///     OptionSpec::new()
///         .short('n')
///         .long("dry-run")
///         .description("print what would be done"),
///     HELP_OPTION,
/// ]);
///
/// static HELP: BuiltinHelp = BuiltinHelp {
///     summary: "do something with files",
///     usage: &["foo [-n] file..."],
///     options: OPTION_SPECS.get(),
///     manual_page_name: "foo",
/// };
///
/// assert_eq!(HELP.summary(), "do something with files");
/// assert_eq!(
///     HELP.message(),
///     "\
/// Usage:
///   foo [-n] file...
///
/// Options:
///   -n, --dry-run  print what would be done
///       --help     print this help
///
/// See <https://magicant.github.io/yash-rs/builtins/foo.html> for details.
/// "
/// );
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuiltinHelp {
    /// One-line summary of the built-in
    pub summary: &'static str,
    /// Usage lines, each showing a form of the command line
    pub usage: &'static [&'static str],
    /// Option specs of the built-in
    ///
    /// For a built-in that accepts `--help`, these should be the specs the
    /// built-in parses its arguments with, obtained by
    /// [`OptionSpecsWithHelp::get`] so that they include [`HELP_OPTION`].
    pub options: &'static [OptionSpec<'static>],
    /// Name of the manual page without the `.html` extension
    pub manual_page_name: &'static str,
}

fn option_name(spec: &OptionSpec) -> String {
    // TODO: Show the name of the option argument
    match (spec.get_short(), spec.get_long()) {
        (Some(short), Some(long)) => format!("-{short}, --{long}"),
        (Some(short), None) => format!("-{short}"),
        (None, Some(long)) => format!("    --{long}"),
        (None, None) => String::new(),
    }
}

impl Help for BuiltinHelp {
    fn summary(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.summary)
    }

    fn message(&self) -> Cow<'_, str> {
        let mut message = String::new();
        if !self.usage.is_empty() {
            message.push_str("Usage:\n");
            for usage in self.usage {
                writeln!(message, "  {usage}").unwrap();
            }
            message.push('\n');
        }

        if !self.options.is_empty() {
            message.push_str("Options:\n");
            let names = self.options.iter().map(option_name).collect::<Vec<_>>();
            let width = names.iter().map(String::len).max().unwrap_or(0);
            for (name, spec) in names.iter().zip(self.options) {
                writeln!(message, "  {name:width$}  {}", spec.get_description()).unwrap();
            }
            message.push('\n');
        }

        writeln!(
            message,
            "See <{MANUAL_BASE_URL}{}.html> for details.",
            self.manual_page_name
        )
        .unwrap();
        Cow::Owned(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_specs_with_help_accepts_help_option_at_end() {
        let specs = &[OptionSpec::new().short('a'), HELP_OPTION];
        assert_eq!(*OptionSpecsWithHelp::new(specs), *specs);
    }

    #[test]
    #[should_panic = "the last option spec should be HELP_OPTION"]
    fn option_specs_with_help_rejects_empty_specs() {
        _ = OptionSpecsWithHelp::new(&[]);
    }

    #[test]
    #[should_panic = "the last option spec should be HELP_OPTION"]
    fn option_specs_with_help_rejects_help_option_not_at_end() {
        _ = OptionSpecsWithHelp::new(&[HELP_OPTION, OptionSpec::new().short('a')]);
    }

    #[test]
    #[should_panic = "the last option spec should be HELP_OPTION"]
    fn option_specs_with_help_rejects_other_help_option() {
        _ = OptionSpecsWithHelp::new(&[OptionSpec::new().long("help")]);
    }

    #[test]
    fn message_with_options() {
        const OPTIONS: &[OptionSpec] = &[
            OptionSpec::new().short('a').description("do A"),
            OptionSpec::new()
                .short('b')
                .long("bravo")
                .description("do B"),
            HELP_OPTION,
        ];
        let help = BuiltinHelp {
            summary: "do something",
            usage: &["foo [-a|-b]"],
            options: OPTIONS,
            manual_page_name: "foo",
        };

        assert_eq!(
            help.message(),
            "Usage:
  foo [-a|-b]

Options:
  -a           do A
  -b, --bravo  do B
      --help   print this help

See <https://magicant.github.io/yash-rs/builtins/foo.html> for details.
"
        );
    }

    #[test]
    fn message_without_options() {
        let help = BuiltinHelp {
            summary: "do something",
            usage: &["foo [arg...]"],
            options: &[],
            manual_page_name: "foo",
        };

        assert_eq!(
            help.message(),
            "Usage:
  foo [arg...]

See <https://magicant.github.io/yash-rs/builtins/foo.html> for details.
"
        );
    }

    #[test]
    fn message_without_usage() {
        let help = BuiltinHelp {
            summary: "do something",
            usage: &[],
            options: &[HELP_OPTION],
            manual_page_name: "foo",
        };

        assert_eq!(
            help.message(),
            "Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/foo.html> for details.
"
        );
    }
}
