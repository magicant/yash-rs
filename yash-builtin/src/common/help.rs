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

use super::output;
use super::syntax::OptionSpec;
use std::fmt::Write as _;
use yash_env::Env;
use yash_env::builtin::Help;
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;

/// Base URL of the manual pages of the built-ins
const MANUAL_BASE_URL: &str = "https://magicant.github.io/yash-rs/builtins/";

/// Spec of the `--help` option
///
/// A built-in that accepts `--help` includes this spec in its option spec
/// list so that the option is recognized by the parser and listed in the help
/// message.
pub const HELP_OPTION: OptionSpec<'static> = OptionSpec::new()
    .long("help")
    .extension(true)
    .description("print this help");

/// Returns whether the option spec is [`HELP_OPTION`].
#[must_use]
pub fn is_help_option(spec: &OptionSpec) -> bool {
    *spec == HELP_OPTION
}

/// Help information of a built-in
///
/// The help message consists of the summary, the usage lines, the option
/// list, and a link to the manual page. The option list is generated from
/// the option specs so that it stays in sync with the parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuiltinHelp {
    /// One-line summary of the built-in
    pub summary: &'static str,
    /// Usage lines, each showing a form of the command line
    pub usage: &'static [&'static str],
    /// Option specs of the built-in
    pub options: &'static [OptionSpec<'static>],
    /// Name of the manual page without the `.html` extension
    pub page: &'static str,
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
    fn summary(&self) -> &str {
        self.summary
    }

    fn message(&self) -> String {
        let mut message = format!("{}\n\nUsage:\n", self.summary);
        for usage in self.usage {
            writeln!(message, "  {usage}").unwrap();
        }

        if !self.options.is_empty() {
            message.push_str("\nOptions:\n");
            let names = self.options.iter().map(option_name).collect::<Vec<_>>();
            let width = names.iter().map(String::len).max().unwrap_or(0);
            for (name, spec) in names.iter().zip(self.options) {
                writeln!(message, "  {name:width$}  {}", spec.get_description()).unwrap();
            }
        }

        writeln!(
            message,
            "\nSee <{MANUAL_BASE_URL}{}.html> for details.",
            self.page
        )
        .unwrap();
        message
    }
}

/// Prints the help message to the standard output.
pub async fn print<S>(env: &mut Env<S>, help: &dyn Help) -> yash_env::builtin::Result
where
    S: Isatty + WriteAll,
{
    output(env, &help.message()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_without_options() {
        let help = BuiltinHelp {
            summary: "do nothing",
            usage: &["foo", "foo bar"],
            options: &[],
            page: "foo",
        };

        assert_eq!(
            help.message(),
            "do nothing

Usage:
  foo
  foo bar

See <https://magicant.github.io/yash-rs/builtins/foo.html> for details.
"
        );
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
            page: "foo",
        };

        assert_eq!(
            help.message(),
            "do something

Usage:
  foo [-a|-b]

Options:
  -a           do A
  -b, --bravo  do B
      --help   print this help

See <https://magicant.github.io/yash-rs/builtins/foo.html> for details.
"
        );
    }
}
