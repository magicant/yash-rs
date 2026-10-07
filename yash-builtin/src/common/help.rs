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
use super::syntax::{Mode, OptionSpec, parse_arguments};
use std::borrow::Cow;
use std::fmt::Write as _;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;

/// Base URL of the manual pages of the built-ins
const MANUAL_BASE_URL: &str = "https://magicant.github.io/yash-rs/builtins/";

/// Help information of a built-in
///
/// An implementation of this trait is registered with
/// [`register`](crate::help::register) so that the shell can print a help
/// message for the built-in.
pub trait Help {
    /// Returns a one-line summary of the built-in.
    fn summary(&self) -> Cow<'_, str>;

    /// Returns the full help message of the built-in.
    ///
    /// The message should end with a newline.
    fn message(&self) -> Cow<'_, str>;
}

/// Spec of the `--help` option
///
/// A built-in that accepts `--help` includes this spec in its option spec
/// list and calls [`print_if_requested`] before parsing its arguments.
pub const HELP_OPTION: OptionSpec<'static> = OptionSpec::new()
    .long("help")
    .extension(true)
    .description("print this help");

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
    fn summary(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.summary)
    }

    fn message(&self) -> Cow<'_, str> {
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
        Cow::Owned(message)
    }
}

/// Prints the help message to the standard output.
pub async fn print<S>(env: &mut Env<S>, help: &dyn Help) -> yash_env::builtin::Result
where
    S: Isatty + WriteAll,
{
    output(env, &help.message()).await
}

/// Runs `help` for the current built-in if `--help` is requested.
///
/// If `args` consist of [`HELP_OPTION`] only, this function runs the `help`
/// built-in with the name of the current built-in as the operand and returns
/// its result. Otherwise, this function returns `None` and the built-in should
/// parse `args` as usual. The option is recognized with `option_specs` so that
/// it can be abbreviated and is rejected in the same way as in
/// [`parse_arguments`].
///
/// The name of the current built-in is taken from
/// [`Stack::current_builtin`](yash_env::stack::Stack::current_builtin).
///
/// # Panics
///
/// If `--help` is requested but there is no built-in in the stack.
pub async fn print_if_requested<S>(
    env: &mut Env<S>,
    option_specs: &[OptionSpec<'_>],
    args: &[Field],
) -> Option<yash_env::builtin::Result>
where
    S: Isatty + WriteAll,
{
    let [arg] = args else { return None };
    // Avoid cloning the argument if it cannot be a long option
    if !arg.value.starts_with("--") {
        return None;
    }
    let (options, _) =
        parse_arguments(option_specs, Mode::with_env(env), vec![arg.clone()]).ok()?;
    if !matches!(options.as_slice(), [option] if *option.spec == HELP_OPTION) {
        return None;
    }

    let name = env
        .stack
        .current_builtin()
        .expect("a built-in frame should be in the stack to tell which built-in --help is for")
        .name
        .clone();
    Some(crate::help::main(env, vec![name]).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::option::{On, Portable};
    use yash_env::stack::{Builtin, Frame};
    use yash_env::system::Concurrent;
    use yash_env::test_helper::assert_stdout;

    fn env() -> (Env<Rc<Concurrent<VirtualSystem>>>, VirtualSystem) {
        let system = VirtualSystem::new();
        let env = Env::with_system(Rc::new(Concurrent::new(system.clone())));
        (env, system)
    }

    fn pwd_frame() -> Frame {
        Frame::Builtin(Builtin {
            name: Field::dummy("pwd"),
            is_special: false,
        })
    }

    #[test]
    fn print_if_requested_prints_help_of_current_builtin() {
        let (mut env, system) = env();
        let mut env = env.push_frame(pwd_frame());
        let args = Field::dummies(["--hel"]);

        let result = print_if_requested(&mut env, &[HELP_OPTION], &args)
            .now_or_never()
            .unwrap();

        assert_eq!(result, Some(yash_env::builtin::Result::default()));
        assert_stdout(&system.state, |stdout| {
            assert_eq!(stdout, crate::pwd::HELP.message())
        });
    }

    #[test]
    fn print_if_requested_ignores_other_arguments() {
        let (mut env, system) = env();
        let mut env = env.push_frame(pwd_frame());
        let specs = &[OptionSpec::new().long("foo"), HELP_OPTION];

        for args in [&[][..], &["--foo"], &["--"], &["-x"], &["--help", "--help"]] {
            let args = Field::dummies(args.iter().copied());
            let result = print_if_requested(&mut env, specs, &args)
                .now_or_never()
                .unwrap();
            assert_eq!(result, None, "args = {args:?}");
        }
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    #[should_panic = "a built-in frame should be in the stack"]
    fn print_if_requested_panics_without_builtin_frame() {
        let (mut env, _) = env();
        let args = Field::dummies(["--help"]);

        _ = print_if_requested(&mut env, &[HELP_OPTION], &args).now_or_never();
    }

    #[test]
    fn print_if_requested_ignores_help_option_in_portable_mode() {
        let (mut env, system) = env();
        let mut env = env.push_frame(pwd_frame());
        env.options.set(Portable, On);
        let args = Field::dummies(["--help"]);

        let result = print_if_requested(&mut env, &[HELP_OPTION], &args)
            .now_or_never()
            .unwrap();

        assert_eq!(result, None);
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
    }

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
