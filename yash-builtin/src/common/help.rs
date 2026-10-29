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
//! This module provides the building blocks for built-ins to have help, which
//! the [`help`](crate::help) built-in prints and which built-ins print when
//! invoked with the `--help` option:
//!
//! - [`Help`] is the trait for the help of a built-in.
//! - [`BuiltinHelp`] is the standard implementation of `Help`, which generates
//!   the help message from structured data, including the option specs the
//!   built-in's parser uses.
//! - [`HELP_OPTION`] is the option spec of `--help`, and
//!   [`OptionSpecsWithHelp`] is a list of option specs that includes it.
//! - [`parse_arguments_with_help`] parses the arguments of a built-in and tells
//!   whether `--help` is requested, in a [`WithHelp`].
//! - [`print_help_of_current_builtin`] prints the help of the built-in that
//!   was requested with `--help`.
//!
//! The help of a built-in is looked up by name in the table maintained by the
//! [`help`](crate::help) module. The table initially contains the help of the
//! built-ins defined in this crate. A built-in defined outside this crate
//! needs to [register](crate::help::register) its help to the table.
//!
//! # Example
//!
//! This is an outline of a built-in that accepts `--help`:
//!
//! ```
//! use yash_builtin::common::help::{
//!     BuiltinHelp, HELP_OPTION, OptionSpecsWithHelp, WithHelp, parse_arguments_with_help,
//!     print_help_of_current_builtin,
//! };
//! use yash_builtin::common::report::report_error;
//! use yash_builtin::common::syntax::{Mode, OptionSpec};
//! use yash_env::Env;
//! use yash_env::semantics::Field;
//! use yash_env::system::Isatty;
//! use yash_env::system::concurrency::WriteAll;
//!
//! // The option specs must end with HELP_OPTION.
//! const OPTION_SPECS: OptionSpecsWithHelp = OptionSpecsWithHelp::new(&[
//!     OptionSpec::new()
//!         .short('n')
//!         .long("dry-run")
//!         .description("print what would be done"),
//!     HELP_OPTION,
//! ]);
//!
//! // The help lists the same options as the built-in accepts.
//! static HELP: BuiltinHelp = BuiltinHelp {
//!     summary: "do something with files",
//!     usage: &["foo [-n] file..."],
//!     options: OPTION_SPECS.get(),
//!     manual_page_name: "foo",
//! };
//!
//! async fn foo_main<S: Isatty + WriteAll>(
//!     env: &mut Env<S>,
//!     args: Vec<Field>,
//! ) -> yash_env::builtin::Result {
//!     let mode = Mode::with_env(env);
//!     match parse_arguments_with_help(OPTION_SPECS, mode, args) {
//!         Ok(WithHelp::Main((options, operands))) => {
//!             // Now perform the built-in's operation with the options and
//!             // operands, which never contain HELP_OPTION.
//! #           drop((options, operands));
//!             Default::default()
//!         }
//!         Ok(WithHelp::Help) => print_help_of_current_builtin(env).await,
//!         Err(error) => report_error(env, &error).await,
//!     }
//! }
//!
//! // When setting up the environment, register the help together with the
//! // built-in itself.
//! fn register_foo<S>(env: &mut Env<S>) {
//!     yash_builtin::help::register(env, "foo", &HELP);
//! }
//! ```

use super::syntax::{
    Mode, OptionArgumentSpec, OptionOccurrence, OptionSpec, ParseError, parse_arguments,
};
use std::borrow::Cow;
use std::fmt::Write as _;
use std::ops::Deref;
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
///
/// [`BuiltinHelp`] is the standard implementation, which formats the help
/// message uniformly from structured data. Implement this trait in your own
/// type if you need to customize the message.
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
/// [`OptionSpecsWithHelp`] and parses its arguments with
/// [`parse_arguments_with_help`].
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
/// of this type and passes it to [`parse_arguments_with_help`] to parse its
/// arguments. The result of [`get`](Self::get) is passed to
/// [`BuiltinHelp::options`] so that the help lists the same options as the
/// built-in accepts. See the [module documentation](self) for an example.
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

/// Result of parsing arguments of a built-in that accepts `--help`
///
/// The parser of such a built-in returns `Help` if `--help` is requested, and
/// `Main` with the result of parsing the other arguments otherwise. When the
/// built-in gets `Help`, it calls [`print_help_of_current_builtin`] instead of
/// performing its main operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WithHelp<C> {
    /// The built-in performs its main operation as specified by the arguments.
    Main(C),
    /// The built-in prints its help.
    Help,
}

/// Parses arguments of a built-in that accepts `--help`.
///
/// This function parses `args` with [`parse_arguments`]. If the arguments
/// consist of [`HELP_OPTION`] only, this function returns
/// [`WithHelp::Help`]. A `--` separator may follow the option. If
/// `HELP_OPTION` is given with other options or operands, this function
/// returns [`ParseError::HelpWithOtherArguments`]. Otherwise, it returns the
/// parsed options and operands in [`WithHelp::Main`], so the returned options
/// never contain `HELP_OPTION`.
///
/// See the [module documentation](self) for an example.
#[allow(clippy::type_complexity, reason = "we can't make this simpler")]
pub fn parse_arguments_with_help<'a>(
    option_specs: OptionSpecsWithHelp<'a>,
    mode: Mode,
    args: Vec<Field>,
) -> Result<WithHelp<(Vec<OptionOccurrence<'a>>, Vec<Field>)>, ParseError<'a>> {
    let (options, operands) = parse_arguments(option_specs.get(), mode, args)?;
    let Some(help) = options.iter().find(|option| *option.spec == HELP_OPTION) else {
        return Ok(WithHelp::Main((options, operands)));
    };
    if options.len() > 1 || !operands.is_empty() {
        let field = Field {
            value: help.spec.to_string(),
            origin: help.location.clone(),
        };
        return Err(ParseError::HelpWithOtherArguments(field));
    }
    Ok(WithHelp::Help)
}

/// Prints the help of the current built-in.
///
/// This function runs the `help` built-in with the name of the current
/// built-in as the operand and returns its result. A built-in calls this
/// function when its parser returns [`WithHelp::Help`].
///
/// The name of the current built-in is taken from
/// [`Stack::current_builtin`](yash_env::stack::Stack::current_builtin). The
/// help is looked up in the table of the [`help`](crate::help) module, so the
/// help of a built-in defined outside this crate must be
/// [registered](crate::help::register) in advance. Otherwise, this function
/// reports that the built-in has no help.
///
/// # Panics
///
/// If there is no built-in in the stack.
pub async fn print_help_of_current_builtin<S>(env: &mut Env<S>) -> yash_env::builtin::Result
where
    S: Isatty + WriteAll,
{
    let name = env
        .stack
        .current_builtin()
        .expect("a built-in frame should be in the stack to tell which built-in --help is for")
        .name
        .clone();
    crate::help::print(env, &[name]).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::stack::{Builtin, Frame};
    use yash_env::system::Concurrent;
    use yash_env::test_helper::assert_stdout;

    fn env() -> (Env<Rc<Concurrent<VirtualSystem>>>, VirtualSystem) {
        let system = VirtualSystem::new();
        let env = Env::with_system(Rc::new(Concurrent::new(system.clone())));
        (env, system)
    }

    const ONLY_HELP: OptionSpecsWithHelp = OptionSpecsWithHelp::new(&[HELP_OPTION]);
    const FOO_AND_HELP: OptionSpecsWithHelp =
        OptionSpecsWithHelp::new(&[OptionSpec::new().long("foo"), HELP_OPTION]);

    fn help_frame() -> Frame {
        Frame::Builtin(Builtin {
            name: Field::dummy("help"),
            is_special: false,
        })
    }

    #[test]
    fn parse_arguments_with_help_returns_help() {
        let args = Field::dummies(["--hel"]);
        let result = parse_arguments_with_help(ONLY_HELP, Mode::with_extensions(), args);
        assert_eq!(result, Ok(WithHelp::Help));
    }

    #[test]
    fn parse_arguments_with_help_accepts_separator_after_help_option() {
        let args = Field::dummies(["--help", "--"]);
        let result = parse_arguments_with_help(ONLY_HELP, Mode::with_extensions(), args);
        assert_eq!(result, Ok(WithHelp::Help));
    }

    #[test]
    fn parse_arguments_with_help_returns_parsed_arguments() {
        let args = Field::dummies(["--foo", "bar"]);
        let result = parse_arguments_with_help(FOO_AND_HELP, Mode::with_extensions(), args);
        assert_matches!(result, Ok(WithHelp::Main((options, operands))) => {
            assert_eq!(options.len(), 1, "options = {options:?}");
            assert_eq!(options[0].spec.get_long(), Some("foo"));
            assert_eq!(operands, Field::dummies(["bar"]));
        });
    }

    #[test]
    fn parse_arguments_with_help_rejects_help_option_with_other_arguments() {
        for args in [
            &["--foo", "--hel"][..],
            &["--hel", "foo"],
            &["--hel", "--", "--"],
        ] {
            let args = Field::dummies(args.iter().copied());
            let location = args
                .iter()
                .find(|arg| arg.value == "--hel")
                .unwrap()
                .origin
                .clone();
            let result = parse_arguments_with_help(FOO_AND_HELP, Mode::with_extensions(), args);
            assert_matches!(result, Err(ParseError::HelpWithOtherArguments(field)) => {
                assert_eq!(field.value, "--help");
                assert_eq!(field.origin, location);
            });
        }
    }

    #[test]
    fn print_help_of_current_builtin_prints_help() {
        let (mut env, system) = env();
        let mut env = env.push_frame(help_frame());

        let result = print_help_of_current_builtin(&mut env)
            .now_or_never()
            .unwrap();

        assert_eq!(result, Default::default());
        assert_stdout(&system.state, |stdout| {
            assert!(stdout.starts_with("help: print the help of built-ins\n\nUsage:\n"))
        });
    }

    #[test]
    #[should_panic = "a built-in frame should be in the stack"]
    fn print_help_of_current_builtin_panics_without_builtin_frame() {
        let (mut env, _) = env();
        _ = print_help_of_current_builtin(&mut env).now_or_never();
    }

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
