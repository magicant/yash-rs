// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Command line argument parser for the set built-in

use super::Command;
use std::iter::Peekable;
use thiserror::Error;
use yash_env::option::FromStrError::*;
use yash_env::option::State;
use yash_env::option::canonicalize;
use yash_env::option::parse_long;
use yash_env::option::parse_short;
use yash_env::semantics::Field;
use yash_env::source::pretty::Snippet;
use yash_env::source::pretty::{Report, ReportType};

/// Error in command line parsing
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum Error {
    /// Short option that is not defined in the option specs
    #[error("unknown option {0:?}")]
    UnknownShortOption(char, Field),

    /// Long option that is not defined in the option specs
    #[error("unknown option {:?}", .0.value)]
    UnknownLongOption(Field),

    /// Long option that matches the prefix of more than one option name.
    #[error("ambiguous option name {:?}", .0.value)]
    AmbiguousLongOption(Field),

    /// `-o` or `+o` used without an option name
    #[error("option {:?} missing an argument", .0.value)]
    MissingOptionArgument(Field),

    /// Short option that is not modifiable by the set built-in
    #[error("option {0:?} not modifiable by the set built-in")]
    UnmodifiableShortOption(char, Field),

    /// Long option that is not modifiable by the set built-in
    #[error("option {:?} not modifiable by the set built-in", .0.value)]
    UnmodifiableLongOption(Field),
}

impl Error {
    /// Returns a reference to the field in which the error occurred.
    pub fn field(&self) -> &Field {
        match self {
            Error::UnknownShortOption(_char, field) => field,
            Error::UnknownLongOption(field) => field,
            Error::AmbiguousLongOption(field) => field,
            Error::MissingOptionArgument(field) => field,
            Error::UnmodifiableShortOption(_char, field) => field,
            Error::UnmodifiableLongOption(field) => field,
        }
    }

    /// Converts this error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();

        let field = self.field();
        report.snippets = Snippet::with_primary_span(&field.origin, field.value.as_str().into());

        report
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(value: &'a Error) -> Self {
        value.to_report()
    }
}

/// Tries to parse the next field in `args`.
///
/// Returns `Ok(true)` if the next field contained a short option, in which case
/// the parsed field is consumed from the iterator.
fn try_parse_short<I: Iterator<Item = Field>>(
    args: &mut Peekable<I>,
    option_occurrences: &mut Vec<(yash_env::option::Option, State)>,
) -> Result<bool, Error> {
    let field = match args.peek() {
        Some(field) => field,
        None => return Ok(false),
    };

    let mut chars = field.value.chars();
    let negate = match chars.next() {
        Some('-') => false,
        Some('+') => true,
        _ => return Ok(false),
    };
    match chars.next() {
        Some('-') if !negate => return Ok(false),
        Some('+') if negate => return Ok(false),
        None => return Ok(false),
        _ => (),
    }

    let mut field = args.next().unwrap();
    let mut chars = field.value.chars();
    chars.next().unwrap();
    while let Some(c) = chars.next() {
        if c == 'o' {
            let name = chars.as_str();
            let name = if !name.is_empty() {
                canonicalize(name)
            } else {
                let prev = field;
                field = args.next().ok_or(Error::MissingOptionArgument(prev))?;
                canonicalize(&field.value)
            };
            match parse_long(&name) {
                Ok((option, state)) if option.is_modifiable() => {
                    option_occurrences.push((option, if negate { !state } else { state }));
                    break;
                }
                Ok(_) => return Err(Error::UnmodifiableLongOption(field)),
                Err(NoSuchOption) => return Err(Error::UnknownLongOption(field)),
                Err(Ambiguous) => return Err(Error::AmbiguousLongOption(field)),
            }
        }

        match parse_short(c) {
            Some((option, state)) if option.is_modifiable() => {
                option_occurrences.push((option, if negate { !state } else { state }))
            }
            Some(_) => return Err(Error::UnmodifiableShortOption(c, field)),
            None => return Err(Error::UnknownShortOption(c, field)),
        }
    }
    Ok(true)
}

/// Tries to parse and consume the next field in `args`.
fn try_parse_long<I: Iterator<Item = Field>>(
    args: &mut Peekable<I>,
) -> Result<std::option::Option<(yash_env::option::Option, State)>, Error> {
    let field = match args.peek() {
        Some(field) => field,
        None => return Ok(None),
    };

    let (name, negate) = if let Some(name) = field.value.strip_prefix("--") {
        if name.is_empty() {
            return Ok(None);
        }
        (name, false)
    } else if let Some(name) = field.value.strip_prefix("++") {
        (name, true)
    } else {
        return Ok(None);
    };

    let name = canonicalize(name);
    let result = parse_long(&name);
    let field = args.next().unwrap();
    match result {
        Ok((option, state)) if option.is_modifiable() => {
            Ok(Some((option, if negate { !state } else { state })))
        }
        Ok(_) => Err(Error::UnmodifiableLongOption(field)),
        Err(NoSuchOption) => Err(Error::UnknownLongOption(field)),
        Err(Ambiguous) => Err(Error::AmbiguousLongOption(field)),
    }
}

/// Parses command line arguments.
pub fn parse(args: Vec<Field>) -> Result<Command, Error> {
    match args.len() {
        0 => return Ok(Command::PrintVariables),
        1 => match args[0].value.as_str() {
            "-o" => return Ok(Command::PrintOptionsHumanReadable),
            "+o" => return Ok(Command::PrintOptionsMachineReadable),
            _ => (),
        },
        _ => (),
    }

    let mut args = args.into_iter().peekable();
    let mut options = Vec::new();
    loop {
        if try_parse_short(&mut args, &mut options)? {
            continue;
        }
        if let Some(result) = try_parse_long(&mut args)? {
            options.push(result);
        } else {
            break;
        }
    }

    let separated = match args.peek().map(|arg| arg.value.as_str()) {
        Some("--" | "-") => {
            drop(args.next());
            true
        }
        _ => false,
    };

    let positional_params = (separated || args.peek().is_some()).then(|| args.collect());

    Ok(Command::Modify {
        options,
        positional_params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use yash_env::option::Option::*;
    use yash_env::option::State::*;

    #[test]
    fn simple_cases() {
        assert_eq!(parse(vec![]), Ok(Command::PrintVariables));
        assert_eq!(
            parse(Field::dummies(["-o"])),
            Ok(Command::PrintOptionsHumanReadable)
        );
        assert_eq!(
            parse(Field::dummies(["+o"])),
            Ok(Command::PrintOptionsMachineReadable)
        );
    }

    #[test]
    fn positional_params_only() {
        assert_matches!(
            parse(Field::dummies(["foo"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first] => {
                    assert_eq!(first.value, "foo");
                });
            }
        );

        assert_matches!(
            parse(Field::dummies([""])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first] => {
                    assert_eq!(first.value, "");
                });
            }
        );

        assert_matches!(
            parse(Field::dummies(["a", "b", "c"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first, second, third] => {
                    assert_eq!(first.value, "a");
                    assert_eq!(second.value, "b");
                    assert_eq!(third.value, "c");
                });
            }
        );
    }

    #[test]
    fn double_hyphen_separator_and_positional_params() {
        assert_matches!(
            parse(Field::dummies(["--"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_eq!(positional_params.unwrap().as_slice(), []);
            }
        );

        assert_matches!(
            parse(Field::dummies(["--", "foo", "bar"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first, second] => {
                    assert_eq!(first.value, "foo");
                    assert_eq!(second.value, "bar");
                });
            }
        );

        assert_matches!(
            parse(Field::dummies(["--", "--"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first] => {
                    assert_eq!(first.value, "--");
                });
            }
        );

        assert_matches!(
            parse(Field::dummies(["--", "-"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first] => {
                    assert_eq!(first.value, "-");
                });
            }
        );

        assert_matches!(
            parse(Field::dummies(["--", "-a"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first] => {
                    assert_eq!(first.value, "-a");
                });
            }
        );
    }

    #[test]
    fn single_hyphen_separator_and_positional_params() {
        assert_matches!(
            parse(Field::dummies(["-"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_eq!(positional_params.unwrap().as_slice(), []);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-", "foo", "bar"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first, second] => {
                    assert_eq!(first.value, "foo");
                    assert_eq!(second.value, "bar");
                });
            }
        );

        assert_matches!(
            parse(Field::dummies(["-", "-"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first] => {
                    assert_eq!(first.value, "-");
                });
            }
        );

        assert_matches!(
            parse(Field::dummies(["-", "--"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first] => {
                    assert_eq!(first.value, "--");
                });
            }
        );

        assert_matches!(
            parse(Field::dummies(["-", "-a"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first] => {
                    assert_eq!(first.value, "-a");
                });
            }
        );
    }

    #[test]
    fn short_options() {
        assert_matches!(
            parse(Field::dummies(["-a"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-uv"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, Off), (Verbose, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-u", "-v"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, Off), (Verbose, On)]);
                assert_eq!(positional_params, None);
            }
        );
    }

    #[test]
    fn negated_short_options() {
        assert_matches!(
            parse(Field::dummies(["+a"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["+uv"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, On), (Verbose, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["+u", "-v"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, On), (Verbose, On)]);
                assert_eq!(positional_params, None);
            }
        );
    }

    #[test]
    fn o_options() {
        assert_matches!(
            parse(Field::dummies(["-oallexpo"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-o all-Expo"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-onounset"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-o","NO_unset"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, Off)]);
                assert_eq!(positional_params, None);
            }
        );
    }

    #[test]
    fn negated_o_options() {
        assert_matches!(
            parse(Field::dummies(["+oallexpo"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["+o all-Expo"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["+onounset"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["+o","NO+unset"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, On)]);
                assert_eq!(positional_params, None);
            }
        );
    }

    #[test]
    fn long_options() {
        assert_matches!(
            parse(Field::dummies(["--allexpo"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-- all-Expo"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["--nounset"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, Off)]);
                assert_eq!(positional_params, None);
            }
        );
    }

    #[test]
    fn negated_long_options() {
        assert_matches!(
            parse(Field::dummies(["++allexpo"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["++ all-Expo"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["++nounset"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, On)]);
                assert_eq!(positional_params, None);
            }
        );
    }

    #[test]
    fn options_and_separator() {
        assert_matches!(
            parse(Field::dummies(["-a", "--"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, Some(vec![]));
            }
        );

        assert_matches!(
            parse(Field::dummies(["-uv", "--", "-a"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, Off), (Verbose, On)]);
                assert_matches!(positional_params.unwrap().as_slice(), [first] => {
                    assert_eq!(first.value, "-a");
                });
            }
        );

        assert_matches!(
            parse(Field::dummies(["-n", "-", "--"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Exec, Off)]);
                assert_matches!(positional_params.unwrap().as_slice(), [first] => {
                    assert_eq!(first.value, "--");
                });
            }
        );
    }

    #[test]
    fn combinations() {
        assert_matches!(
            parse(Field::dummies(["+nononotify", "a", "-a"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Exec, On), (Notify, On)]);
                assert_matches!(positional_params.unwrap().as_slice(), [first, second] => {
                    assert_eq!(first.value, "a");
                    assert_eq!(second.value, "-a");
                });
            }
        );

        assert_matches!(
            parse(Field::dummies(["-uno", "-notify", "++log", "--", "foo", "-v"])),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, Off), (Exec, Off), (Notify, On), (Log, Off)]);
                assert_matches!(positional_params.unwrap().as_slice(), [first, second] => {
                    assert_eq!(first.value, "foo");
                    assert_eq!(second.value, "-v");
                });
            }
        );
    }

    #[test]
    fn parse_errors() {
        assert_matches!(
            parse(Field::dummies(["-n-a"])),
            Err(Error::UnknownShortOption('-', field)) => {
                assert_eq!(field.value, "-n-a");
            }
        );

        assert_matches!(
            parse(Field::dummies(["--foo"])),
            Err(Error::UnknownLongOption(field)) => {
                assert_eq!(field.value, "--foo");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-ofoo"])),
            Err(Error::UnknownLongOption(field)) => {
                assert_eq!(field.value, "-ofoo");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-o", "foo"])),
            Err(Error::UnknownLongOption(field)) => {
                assert_eq!(field.value, "foo");
            }
        );

        assert_matches!(
            parse(Field::dummies(["--no"])),
            Err(Error::AmbiguousLongOption(field)) => {
                assert_eq!(field.value, "--no");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-oe"])),
            Err(Error::AmbiguousLongOption(field)) => {
                assert_eq!(field.value, "-oe");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-eo"])),
            Err(Error::MissingOptionArgument(field)) => {
                assert_eq!(field.value, "-eo");
            }
        );
    }

    #[test]
    fn unmodifiable_options() {
        assert_matches!(
            parse(Field::dummies(["-c"])),
            Err(Error::UnmodifiableShortOption('c', field)) => {
                assert_eq!(field.value, "-c");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-ointeract"])),
            Err(Error::UnmodifiableLongOption(field)) => {
                assert_eq!(field.value, "-ointeract");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-o", "interact"])),
            Err(Error::UnmodifiableLongOption(field)) => {
                assert_eq!(field.value, "interact");
            }
        );

        assert_matches!(
            parse(Field::dummies(["++stdin"])),
            Err(Error::UnmodifiableLongOption(field)) => {
                assert_eq!(field.value, "++stdin");
            }
        );
    }
}
