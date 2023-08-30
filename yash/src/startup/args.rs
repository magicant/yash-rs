// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Command line argument parser for the shell

use std::iter::Peekable;
use thiserror::Error;
use yash_env::option::canonicalize;
use yash_env::option::parse_long;
use yash_env::option::parse_short;
use yash_env::option::FromStrError::{Ambiguous, NoSuchOption};
use yash_env::option::Option as ShellOption;
use yash_env::option::State;
#[cfg(doc)]
use yash_env::Env;

/// Input to the main read-eval loop
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum Source {
    /// Read from standard input (the `-s` option)
    #[default]
    Stdin,
    /// Read from a file (no option)
    File { path: String },
    /// Read from a string (the `-c` option)
    String(String),
}

/// Option specifying an initialization file
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum InitFile {
    /// No initialization file
    None,
    /// Use the default initialization file
    #[default]
    Default,
    /// Use the specified initialization file
    File { path: String },
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
/// Configuration for starting the main read-eval loop
pub struct Run {
    /// Input source
    pub source: Source,
    /// Initialization file for a login shell
    pub profile: InitFile,
    /// Initialization file for an interactive shell
    pub rcfile: InitFile,
    /// Shell options
    pub options: Vec<(ShellOption, State)>,
    /// Value of [`Env::arg0`]
    pub arg0: String,
    /// Positional parameters
    pub positional_params: Vec<String>,
}

/// Parse result
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Parse {
    /// Runs the shell
    Run(Run),
    /// Prints help message and exit
    Help,
    /// Prints version information and exit
    Version,
}

impl From<Run> for Parse {
    fn from(run: Run) -> Self {
        Parse::Run(run)
    }
}

/// Error in command line parsing
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum Error {
    /// Short option that is not defined in the option specs
    #[error("unknown option `{0}`")]
    UnknownShortOption(char),

    /// Long option that is not defined in the option specs
    #[error("unknown option `{0}`")]
    UnknownLongOption(String),

    /// Long option that matches the prefix of more than one option name.
    #[error("ambiguous option name `{0}`")]
    AmbiguousLongOption(String),

    /// Option missing an argument
    #[error("option `{0}` missing an argument")]
    MissingOptionArgument(String),

    /// Argument specified to an option that does not take an argument
    #[error("option `{0}` does not take an argument")]
    UnexpectedOptionArgument(String),

    /// The `-c` and `-s` options used together
    #[error("cannot specify both `-c` and `-s`")]
    ConflictingSources,

    /// Negated short option that is not a shell option
    #[error("cannot negate option `{0}`")]
    UnnegatableShortOption(char),

    /// Negated long option that is not a shell option
    #[error("cannot negate option `{0}`")]
    UnnegatableLongOption(String),

    /// The `-c` option without a command string
    #[error("missing command string for `-c`")]
    MissingCommandString,
}

/// Result of parsing short options
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShortOption {
    /// One or more shell options
    Shell,
    /// The `-V` option
    Version,
}

/// Result of parsing a long option
#[derive(Clone, Debug, PartialEq, Eq)]
enum LongOption {
    Shell(ShellOption, State),
    Profile { path: String },
    NoProfile,
    Rcfile { path: String },
    NoRcfile,
    Help,
    Version,
}

/// Intermediate object for parsing a long option
#[derive(Clone, Debug, PartialEq, Eq)]
enum NonShellOptionConstructor {
    WithoutArgument(LongOption),
    WithArgument(fn(String) -> LongOption),
}

impl NonShellOptionConstructor {
    fn from_name(name: &str) -> Option<Self> {
        if "profile".starts_with(name) {
            Some(Self::WithArgument(|path| LongOption::Profile { path }))
        } else if "rcfile".starts_with(name) {
            Some(Self::WithArgument(|path| LongOption::Rcfile { path }))
        } else if "noprofile".starts_with(name) {
            Some(Self::WithoutArgument(LongOption::NoProfile))
        } else if "norcfile".starts_with(name) {
            Some(Self::WithoutArgument(LongOption::NoRcfile))
        } else if "help".starts_with(name) {
            Some(Self::WithoutArgument(LongOption::Help))
        } else if "version".starts_with(name) {
            Some(Self::WithoutArgument(LongOption::Version))
        } else {
            None
        }
    }
}

/// Parses command line arguments.
pub fn parse<I, S>(args: I) -> Result<Parse, Error>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = args.into_iter().map(Into::into).peekable();
    let mut result = Run::default();

    // Below, we use `args.next_if(|_| true)` instead of `args.next()` to avoid
    // consuming the `None` value that needs to be seen again.

    // Parse the command name
    if let Some(arg0) = args.next_if(|_| true) {
        parse_arg0(&arg0, &mut result.options);
        result.arg0 = arg0;
    }

    // Parse options
    loop {
        if let Some(option) = try_parse_short(&mut args, &mut result.options)? {
            match option {
                ShortOption::Shell => continue,
                ShortOption::Version => return Ok(Parse::Version),
            }
        }

        let Some(option) = try_parse_long(&mut args)? else { break; };
        match option {
            LongOption::Shell(option, state) => result.options.push((option, state)),
            LongOption::Profile { path } => {
                if result.profile != InitFile::None {
                    result.profile = InitFile::File { path }
                }
            }
            LongOption::NoProfile => result.profile = InitFile::None,
            LongOption::Rcfile { path } => {
                if result.rcfile != InitFile::None {
                    result.rcfile = InitFile::File { path }
                }
            }
            LongOption::NoRcfile => result.rcfile = InitFile::None,
            LongOption::Help => return Ok(Parse::Help),
            LongOption::Version => return Ok(Parse::Version),
        }
    }

    args.next_if(|arg| arg == "-" || arg == "--");

    // Parse operands
    if result.options.contains(&(ShellOption::CmdLine, State::On)) {
        if result.options.contains(&(ShellOption::Stdin, State::On)) {
            return Err(Error::ConflictingSources);
        }

        let command = args.next_if(|_| true).ok_or(Error::MissingCommandString)?;
        result.source = Source::String(command);
        if let Some(name) = args.next_if(|_| true) {
            result.arg0 = name;
        }
    } else if result.options.contains(&(ShellOption::Stdin, State::On)) {
        result.source = Source::Stdin;
    } else {
        // No -c or -s
        if let Some(operand) = args.next_if(|_| true) {
            result.arg0 = operand.clone();
            result.source = Source::File { path: operand };
        }
    }
    result.positional_params = args.collect();

    Ok(Parse::Run(result))
}

fn parse_arg0(arg0: &str, options: &mut Vec<(ShellOption, State)>) {
    if arg0.starts_with('-') {
        options.push((ShellOption::Login, State::On));
    }
    if arg0.rsplit('/').next().unwrap_or("") == "sh" {
        options.push((ShellOption::PosixlyCorrect, State::On));
    }
}

/// Parses the next argument as short options.
///
/// If the next argument is not a short option, returns `Ok(None)`.
/// If the next argument is a short option, consumes it and returns `Ok(Some(_))`.
/// The parsed options are added to `option_occurrences`.
/// If the `-V` option is included, returns `Ok(Some(ShortOption::Version))`.
fn try_parse_short<I: Iterator<Item = String>>(
    args: &mut Peekable<I>,
    option_occurrences: &mut Vec<(ShellOption, State)>,
) -> Result<Option<ShortOption>, Error> {
    let Some(mut arg) = args.next_if(|arg| is_short_option(arg)) else {
        return Ok(None);
    };

    let mut chars = arg.chars();
    let negate = match chars.next() {
        Some('-') => false,
        Some('+') => true,
        _ => unreachable!(),
    };

    while let Some(c) = chars.next() {
        if c == 'V' {
            return if negate {
                Err(Error::UnnegatableShortOption('V'))
            } else {
                Ok(Some(ShortOption::Version))
            };
        }
        if c == 'o' {
            let name = chars.as_str();
            let name = if !name.is_empty() {
                canonicalize(name)
            } else {
                let prev = arg;
                arg = args.next().ok_or(Error::MissingOptionArgument(prev))?;
                canonicalize(&arg)
            };
            match parse_long(&name) {
                Ok((option, state)) => {
                    option_occurrences.push((option, if negate { !state } else { state }));
                    break;
                }
                Err(NoSuchOption) => return Err(Error::UnknownLongOption(name.into_owned())),
                Err(Ambiguous) => return Err(Error::AmbiguousLongOption(name.into_owned())),
            }
        }

        let (option, state) = parse_short(c).ok_or(Error::UnknownShortOption(c))?;
        option_occurrences.push((option, if negate { !state } else { state }));
    }

    Ok(Some(ShortOption::Shell))
}

/// Tests if the given string is a short option.
fn is_short_option(arg: &str) -> bool {
    let mut chars = arg.chars();
    let negate = match chars.next() {
        Some('-') => false,
        Some('+') => true,
        _ => return false,
    };
    match chars.next() {
        Some('-') if !negate => false,
        Some('+') if negate => false,
        Some(_) => true,
        None => false,
    }
}

/// Tries to parse and consume the next argument in `args` as a long option.
fn try_parse_long<I: Iterator<Item = String>>(
    args: &mut Peekable<I>,
) -> Result<Option<LongOption>, Error> {
    let Some(arg) = args.next_if(|arg| is_long_option(arg)) else {
        return Ok(None);
    };

    let mut chars = arg.chars();
    let negate = match chars.next() {
        Some('-') => false,
        Some('+') => true,
        _ => unreachable!(),
    };

    // Skip the second `-` or `+`
    chars.next();

    let chars = chars.as_str();

    // Parse non-shell options
    let (name, value) = match chars.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (chars, None),
    };
    let non_shell_option = NonShellOptionConstructor::from_name(name);

    // Parse shell options
    let shell_option = parse_long(&canonicalize(chars));

    // Check if the result is unique and return the final result
    match (non_shell_option, shell_option) {
        (_, Err(Ambiguous)) | (Some(_), Ok(_)) => Err(Error::AmbiguousLongOption(arg)),

        (None, Err(NoSuchOption)) => Err(Error::UnknownLongOption(arg)),

        (Some(_), Err(NoSuchOption)) if negate => Err(Error::UnnegatableLongOption(arg)),

        (Some(NonShellOptionConstructor::WithoutArgument(option)), Err(NoSuchOption)) => {
            if value.is_none() {
                Ok(Some(option))
            } else {
                Err(Error::UnexpectedOptionArgument(arg))
            }
        }

        (Some(NonShellOptionConstructor::WithArgument(ctor)), Err(NoSuchOption)) => {
            let value = match value {
                Some(value) => value.to_owned(),
                None => match args.next() {
                    Some(next_arg) => next_arg,
                    None => return Err(Error::MissingOptionArgument(arg)),
                },
            };
            Ok(Some(ctor(value)))
        }

        (None, Ok((option, state))) if negate => Ok(Some(LongOption::Shell(option, !state))),
        (None, Ok((option, state))) => Ok(Some(LongOption::Shell(option, state))),
    }
}

/// Tests if the given string is a long option.
fn is_long_option(arg: &str) -> bool {
    if let Some(name) = arg.strip_prefix("--") {
        !name.is_empty()
    } else {
        arg.starts_with("++")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    fn parse<I, S>(args: I) -> Result<Parse, Error>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        use fuzed_iterator::IteratorExt;
        super::parse(args.into_iter().fuze())
    }

    #[test]
    fn no_arguments() {
        assert_eq!(parse([] as [&str; 0]), Ok(Parse::Run(Run::default())));
    }

    #[test]
    fn arg0_only() {
        assert_eq!(
            parse(["yash"]),
            Ok(Parse::Run(Run {
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn run_file() {
        // Without positional parameters
        assert_eq!(
            parse(["yash", "my-script"]),
            Ok(Parse::Run(Run {
                source: Source::File {
                    path: "my-script".to_string()
                },
                arg0: "my-script".to_string(),
                ..Run::default()
            })),
        );

        // With positional parameters
        assert_eq!(
            parse(["yash", "path/to/script", "-option", "foo", "bar"]),
            Ok(Parse::Run(Run {
                source: Source::File {
                    path: "path/to/script".to_string()
                },
                arg0: "path/to/script".to_string(),
                positional_params: vec![
                    "-option".to_string(),
                    "foo".to_string(),
                    "bar".to_string()
                ],
                ..Run::default()
            })),
        );
    }

    #[test]
    fn run_string() {
        // Without command name or positional parameters
        assert_eq!(
            parse(["yash", "-c", "echo"]),
            Ok(Parse::Run(Run {
                source: Source::String("echo".to_string()),
                options: vec![(ShellOption::CmdLine, State::On)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // With command name but no positional parameters
        assert_eq!(
            parse(["yash", "-c", "echo", "name"]),
            Ok(Parse::Run(Run {
                source: Source::String("echo".to_string()),
                options: vec![(ShellOption::CmdLine, State::On)],
                arg0: "name".to_string(),
                ..Run::default()
            })),
        );

        // With command name and positional parameters
        assert_eq!(
            parse(["yash", "-c", "echo", "name", "foo", "bar"]),
            Ok(Parse::Run(Run {
                source: Source::String("echo".to_string()),
                options: vec![(ShellOption::CmdLine, State::On)],
                arg0: "name".to_string(),
                positional_params: vec!["foo".to_string(), "bar".to_string()],
                ..Run::default()
            }))
        );

        // long option
        assert_eq!(
            parse(["yash", "--cmd-line", "echo"]),
            Ok(Parse::Run(Run {
                source: Source::String("echo".to_string()),
                options: vec![(ShellOption::CmdLine, State::On)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn missing_command_string() {
        assert_eq!(parse(["yash", "-c"]), Err(Error::MissingCommandString));
    }

    #[test]
    fn run_stdin() {
        // Without positional parameters
        assert_eq!(
            parse(["yash", "-s"]),
            Ok(Parse::Run(Run {
                source: Source::Stdin,
                options: vec![(ShellOption::Stdin, State::On)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // With positional parameters
        assert_eq!(
            parse(["yash", "-s", "foo", "bar", "-baz"]),
            Ok(Parse::Run(Run {
                source: Source::Stdin,
                options: vec![(ShellOption::Stdin, State::On)],
                arg0: "yash".to_string(),
                positional_params: vec!["foo".to_string(), "bar".to_string(), "-baz".to_string()],
                ..Run::default()
            })),
        );

        // long option
        assert_eq!(
            parse(["yash", "--stdin"]),
            Ok(Parse::Run(Run {
                source: Source::Stdin,
                options: vec![(ShellOption::Stdin, State::On)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn conflicting_sources() {
        assert_eq!(parse(["yash", "-cs"]), Err(Error::ConflictingSources));
    }

    #[test]
    fn short_options() {
        // Single short option
        assert_eq!(
            parse(["yash", "-a"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::AllExport, State::On)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
        assert_eq!(
            parse(["yash", "-n"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::Exec, State::Off)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Combined short options
        assert_eq!(
            parse(["yash", "-an"]),
            Ok(Parse::Run(Run {
                options: vec![
                    (ShellOption::AllExport, State::On),
                    (ShellOption::Exec, State::Off)
                ],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Many short options
        assert_eq!(
            parse(["yash", "-a", "-nu", "-x"]),
            Ok(Parse::Run(Run {
                options: vec![
                    (ShellOption::AllExport, State::On),
                    (ShellOption::Exec, State::Off),
                    (ShellOption::Unset, State::Off),
                    (ShellOption::XTrace, State::On),
                ],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn negated_short_options() {
        // Single short option
        assert_eq!(
            parse(["yash", "+a"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::AllExport, State::Off)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
        assert_eq!(
            parse(["yash", "+n"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::Exec, State::On)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Combined short options
        assert_eq!(
            parse(["yash", "+an"]),
            Ok(Parse::Run(Run {
                options: vec![
                    (ShellOption::AllExport, State::Off),
                    (ShellOption::Exec, State::On)
                ],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Many short options
        assert_eq!(
            parse(["yash", "+a", "+ns", "+x"]),
            Ok(Parse::Run(Run {
                options: vec![
                    (ShellOption::AllExport, State::Off),
                    (ShellOption::Exec, State::On),
                    (ShellOption::Stdin, State::Off),
                    (ShellOption::XTrace, State::Off),
                ],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn o_options() {
        // Adjoined o options
        assert_eq!(
            parse(["yash", "-oallexport"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::AllExport, State::On)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Separate o options
        assert_eq!(
            parse(["yash", "-o", "allexport"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::AllExport, State::On)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Non-canonical o options
        assert_eq!(
            parse(["yash", "-o", "all-Export", "-o StD+in_"]),
            Ok(Parse::Run(Run {
                options: vec![
                    (ShellOption::AllExport, State::On),
                    (ShellOption::Stdin, State::On),
                ],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn negated_o_options() {
        // Adjoined o options
        assert_eq!(
            parse(["yash", "+oallexport"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::AllExport, State::Off)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Separate o options
        assert_eq!(
            parse(["yash", "+o", "allexport"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::AllExport, State::Off)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Non-canonical o options
        assert_eq!(
            parse(["yash", "+o", "all-Export", "+o no_exec"]),
            Ok(Parse::Run(Run {
                options: vec![
                    (ShellOption::AllExport, State::Off),
                    (ShellOption::Exec, State::On),
                ],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn long_options() {
        assert_eq!(
            parse(["yash", "--all-export"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::AllExport, State::On)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
        assert_eq!(
            parse(["yash", "--all-export", "--no*un=set"]),
            Ok(Parse::Run(Run {
                options: vec![
                    (ShellOption::AllExport, State::On),
                    (ShellOption::Unset, State::Off),
                ],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn negated_long_options() {
        assert_eq!(
            parse(["yash", "++all-export"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::AllExport, State::Off)],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
        assert_eq!(
            parse(["yash", "++all+export", "++no*un-set"]),
            Ok(Parse::Run(Run {
                options: vec![
                    (ShellOption::AllExport, State::Off),
                    (ShellOption::Unset, State::On),
                ],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn profile_option() {
        // Separate argument
        assert_eq!(
            parse(["yash", "--profile", "my/file"]),
            Ok(Parse::Run(Run {
                profile: InitFile::File {
                    path: "my/file".to_string(),
                },
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Adjoined argument
        assert_eq!(
            parse(["yash", "--profile=my/file"]),
            Ok(Parse::Run(Run {
                profile: InitFile::File {
                    path: "my/file".to_string(),
                },
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Abbreviated option name
        assert_eq!(
            parse(["yash", "--pr=ofile"]),
            Ok(Parse::Run(Run {
                profile: InitFile::File {
                    path: "ofile".to_string(),
                },
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn missing_profile_option_argument() {
        assert_eq!(
            parse(["yash", "--profile"]),
            Err(Error::MissingOptionArgument("--profile".to_string())),
        );
    }

    #[test]
    fn noprofile_option() {
        let expected = Ok(Parse::Run(Run {
            profile: InitFile::None,
            arg0: "yash".to_string(),
            ..Run::default()
        }));
        assert_eq!(parse(["yash", "--noprofile"]), expected);

        // noprofile option wins over profile option
        assert_eq!(parse(["yash", "--profile=file", "--noprofile"]), expected);
        assert_eq!(parse(["yash", "--noprofile", "--profile=file"]), expected);
    }

    #[test]
    fn unexpected_noprofile_option_argument() {
        assert_eq!(
            parse(["yash", "--noprofile=x"]),
            Err(Error::UnexpectedOptionArgument("--noprofile=x".to_string())),
        );
    }

    #[test]
    fn rcfile_option() {
        // Separate argument
        assert_eq!(
            parse(["yash", "--rcfile", "my/file"]),
            Ok(Parse::Run(Run {
                rcfile: InitFile::File {
                    path: "my/file".to_string(),
                },
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Adjoined argument
        assert_eq!(
            parse(["yash", "--rcfile=my/file"]),
            Ok(Parse::Run(Run {
                rcfile: InitFile::File {
                    path: "my/file".to_string(),
                },
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        // Abbreviated option name
        assert_eq!(
            parse(["yash", "--rc=file"]),
            Ok(Parse::Run(Run {
                rcfile: InitFile::File {
                    path: "file".to_string(),
                },
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn missing_rcfile_option_argument() {
        assert_eq!(
            parse(["yash", "--rcfile"]),
            Err(Error::MissingOptionArgument("--rcfile".to_string())),
        );
    }

    #[test]
    fn norcfile_option() {
        let expected = Ok(Parse::Run(Run {
            rcfile: InitFile::None,
            arg0: "yash".to_string(),
            ..Run::default()
        }));
        assert_eq!(parse(["yash", "--norcfile"]), expected);

        // norcfile option wins over rcfile option
        assert_eq!(parse(["yash", "--rcfile=file", "--norcfile"]), expected);
        assert_eq!(parse(["yash", "--norcfile", "--rcfile=file"]), expected);
    }

    #[test]
    fn option_combinations() {
        assert_eq!(
            parse(["yash", "-a", "--err-exit", "-u"]),
            Ok(Parse::Run(Run {
                options: vec![
                    (ShellOption::AllExport, State::On),
                    (ShellOption::ErrExit, State::On),
                    (ShellOption::Unset, State::Off),
                ],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        assert_eq!(
            parse(["yash", "-xo", "noclobber", "-il"]),
            Ok(Parse::Run(Run {
                options: vec![
                    (ShellOption::XTrace, State::On),
                    (ShellOption::Clobber, State::Off),
                    (ShellOption::Interactive, State::On),
                    (ShellOption::Login, State::On),
                ],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );

        assert_eq!(
            parse(["yash", "--all", "-f", "--posix"]),
            Ok(Parse::Run(Run {
                options: vec![
                    (ShellOption::AllExport, State::On),
                    (ShellOption::Glob, State::Off),
                    (ShellOption::PosixlyCorrect, State::On),
                ],
                arg0: "yash".to_string(),
                ..Run::default()
            })),
        );
    }

    #[test]
    fn double_hyphen_separator_and_operands() {
        assert_eq!(
            parse(["yash", "--"]),
            Ok(Parse::Run(Run {
                arg0: "yash".to_string(),
                ..Default::default()
            })),
        );

        assert_eq!(
            parse(["yash", "-a", "--", "file", "arg"]),
            Ok(Parse::Run(Run {
                source: Source::File {
                    path: "file".to_string()
                },
                options: vec![(ShellOption::AllExport, State::On)],
                arg0: "file".to_string(),
                positional_params: vec!["arg".to_string()],
                ..Run::default()
            })),
        );

        assert_eq!(
            parse(["yash", "-a", "--", "--", "arg"]),
            Ok(Parse::Run(Run {
                source: Source::File {
                    path: "--".to_string()
                },
                options: vec![(ShellOption::AllExport, State::On)],
                arg0: "--".to_string(),
                positional_params: vec!["arg".to_string()],
                ..Run::default()
            })),
        );
    }

    #[test]
    fn single_hyphen_separator_and_operands() {
        assert_eq!(
            parse(["yash", "-"]),
            Ok(Parse::Run(Run {
                arg0: "yash".to_string(),
                ..Default::default()
            })),
        );

        assert_eq!(
            parse(["yash", "-a", "-", "file", "arg"]),
            Ok(Parse::Run(Run {
                source: Source::File {
                    path: "file".to_string()
                },
                options: vec![(ShellOption::AllExport, State::On)],
                arg0: "file".to_string(),
                positional_params: vec!["arg".to_string()],
                ..Run::default()
            })),
        );

        assert_eq!(
            parse(["yash", "-a", "-", "-", "arg"]),
            Ok(Parse::Run(Run {
                source: Source::File {
                    path: "-".to_string()
                },
                options: vec![(ShellOption::AllExport, State::On)],
                arg0: "-".to_string(),
                positional_params: vec!["arg".to_string()],
                ..Run::default()
            })),
        );
    }

    #[test]
    fn option_after_operand() {
        assert_eq!(
            parse(["yash", "-a", "file", "-e"]),
            Ok(Parse::Run(Run {
                source: Source::File {
                    path: "file".to_string()
                },
                options: vec![(ShellOption::AllExport, State::On)],
                arg0: "file".to_string(),
                positional_params: vec!["-e".to_string()],
                ..Run::default()
            })),
        );
    }

    #[test]
    fn leading_hyphen_in_arg0_makes_login_shell() {
        assert_eq!(
            parse(["-yash"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::Login, State::On)],
                arg0: "-yash".to_string(),
                ..Run::default()
            })),
        );

        assert_matches!(
            parse(["-/bin/sh"]),
            Ok(Parse::Run(run)) => {
                assert!(run.options.contains(&(ShellOption::Login, State::On)));
            }
        );
    }

    #[test]
    fn command_name_sh_enables_posix_mode() {
        assert_eq!(
            parse(["sh"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::PosixlyCorrect, State::On)],
                arg0: "sh".to_string(),
                ..Run::default()
            })),
        );
        assert_eq!(
            parse(["/usr/bin/sh"]),
            Ok(Parse::Run(Run {
                options: vec![(ShellOption::PosixlyCorrect, State::On)],
                arg0: "/usr/bin/sh".to_string(),
                ..Run::default()
            })),
        );

        assert_matches!(
            parse(["-/bin/sh"]),
            Ok(Parse::Run(run)) => {
                assert!(run.options.contains(&(ShellOption::PosixlyCorrect, State::On)));
            }
        );
    }

    #[test]
    fn help_option() {
        assert_eq!(parse(["yash", "--help"]), Ok(Parse::Help));
        assert_eq!(parse(["yash", "-a", "--help", "file"]), Ok(Parse::Help));
    }

    #[test]
    fn version_option() {
        assert_eq!(parse(["yash", "-V"]), Ok(Parse::Version));
        assert_eq!(parse(["yash", "-aV", "x"]), Ok(Parse::Version));

        assert_eq!(parse(["yash", "--version"]), Ok(Parse::Version));
        assert_eq!(parse(["yash", "-a", "--version", "x"]), Ok(Parse::Version));
    }

    #[test]
    fn ambiguous_long_option() {
        assert_eq!(
            parse(["yash", "--no"]),
            Err(Error::AmbiguousLongOption("--no".to_string())),
        );
        assert_eq!(
            parse(["yash", "--p"]),
            Err(Error::AmbiguousLongOption("--p".to_string())),
        );
        assert_eq!(
            parse(["yash", "--ver=bose"]),
            Err(Error::AmbiguousLongOption("--ver=bose".to_string())),
        );
    }

    #[test]
    fn non_existing_option() {
        assert_eq!(
            parse(["yash", "-x", "-y"]),
            Err(Error::UnknownShortOption('y')),
        );
        assert_eq!(parse(["yash", "-CDf"]), Err(Error::UnknownShortOption('D')),);

        assert_eq!(
            parse(["yash", "--unexisting"]),
            Err(Error::UnknownLongOption("--unexisting".to_string())),
        );
        assert_eq!(
            parse(["yash", "--no+un=existing"]),
            Err(Error::UnknownLongOption("--no+un=existing".to_string())),
        );
    }

    #[test]
    fn unnegatable_short_option() {
        assert_eq!(
            parse(["yash", "+V"]),
            Err(Error::UnnegatableShortOption('V')),
        );
    }

    #[test]
    fn unnegatable_long_option() {
        assert_eq!(
            parse(["yash", "++profile"]),
            Err(Error::UnnegatableLongOption("++profile".to_string())),
        );
        assert_eq!(
            parse(["yash", "++vers=ion"]),
            Err(Error::UnnegatableLongOption("++vers=ion".to_string())),
        );
    }
}
