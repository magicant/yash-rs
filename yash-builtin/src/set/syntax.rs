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
use yash_env::option::Option::Portable;
use yash_env::option::State;
use yash_env::option::canonicalize;
use yash_env::option::parse_long;
use yash_env::option::parse_short;
use yash_env::semantics::Field;
use yash_env::source::pretty::Snippet;
use yash_env::source::pretty::{Footnote, FootnoteType};
use yash_env::source::pretty::{Report, ReportType};
use yash_quote::quoted;

/// Error in command line parsing
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
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

    /// Short option that POSIX does not specify for the set built-in
    ///
    /// This error occurs only while the `portable` shell option is on.
    #[error("non-portable option {0:?}")]
    NonPortableShortOption(char, Field),

    /// Option name that POSIX does not specify, or that POSIX spells
    /// differently
    ///
    /// This error occurs only while the `portable` shell option is on. It
    /// covers the `--name` and `++name` forms, which POSIX does not define at
    /// all, as well as an `-o` or `+o` argument that is not an exact POSIX
    /// option name.
    ///
    /// The second item is the option the name denotes and the state the user
    /// requested, if the name denotes one. It is used to suggest a portable
    /// spelling.
    #[error("non-portable option name {:?}", .0.value)]
    NonPortableLongOption(
        Field,
        std::option::Option<(yash_env::option::Option, State)>,
    ),

    /// `-` used as a separator between options and operands
    ///
    /// POSIX does not specify `-` as an option-operand separator, so this
    /// error occurs only while the `portable` shell option is on.
    #[error("non-portable option-operand separator {:?}", .0.value)]
    NonPortableSeparator(Field),

    /// `-o` or `+o` whose argument is not a separate field
    ///
    /// POSIX requires conforming applications to specify an option argument as
    /// a separate argument, so this error occurs only while the `portable`
    /// shell option is on. The second item is the separated spelling of the
    /// field.
    #[error("option argument not separated from the option name in {:?}", .0.value)]
    UnseparatedOptionArgument(Field, String),
}

/// Returns whether the given `-o` option name is one POSIX specifies.
///
/// `name` is the name as written by the user, before [canonicalization](canonicalize),
/// and `state` is the state the name renders, before any negation by `+o`.
fn is_portable_long_name(name: &str, option: yash_env::option::Option, state: State) -> bool {
    if option == Portable {
        // POSIX does not specify the `portable` option, but it must remain
        // possible to turn it off again once it is on. Accept its canonical
        // spelling only.
        name == "portable"
    } else {
        option.portable_long_name() == Some((name, state))
    }
}

/// Returns a portable spelling that would set the option to the given state.
///
/// The result is `None` if POSIX specifies no name for the option.
fn portable_spelling(
    option: yash_env::option::Option,
    state: State,
) -> std::option::Option<String> {
    fn flag(state: State, name_state: State) -> char {
        if state == name_state { '-' } else { '+' }
    }

    if option == Portable {
        // See `is_portable_long_name` for why this spelling is accepted.
        return Some(format!("{}o portable", flag(state, State::On)));
    }
    if let Some((name, name_state)) = option.portable_long_name() {
        return Some(format!("{}o {name}", flag(state, name_state)));
    }
    let (name, name_state) = option.portable_short_name()?;
    Some(format!("{}{name}", flag(state, name_state)))
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
            Error::NonPortableShortOption(_char, field) => field,
            Error::NonPortableLongOption(field, _option) => field,
            Error::NonPortableSeparator(field) => field,
            Error::UnseparatedOptionArgument(field, _spelling) => field,
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

        if let Error::NonPortableShortOption(..)
        | Error::NonPortableLongOption(..)
        | Error::NonPortableSeparator(..)
        | Error::UnseparatedOptionArgument(..) = self
        {
            report.footnotes.push(Footnote {
                r#type: FootnoteType::Note,
                label: "this error is reported because the `portable` shell option is enabled"
                    .into(),
            });
        }

        let spelling = match self {
            Error::NonPortableLongOption(_field, Some((option, state))) => {
                portable_spelling(*option, *state)
            }
            Error::NonPortableSeparator(_field) => Some("--".to_string()),
            Error::UnseparatedOptionArgument(_field, spelling) => Some(spelling.clone()),
            _ => None,
        };
        if let Some(spelling) = spelling {
            report.footnotes.push(Footnote {
                r#type: FootnoteType::Suggestion,
                label: format!("use `{spelling}` instead").into(),
            });
        }

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
///
/// `portable` is the state of the [`Portable`] option in effect for this field.
/// It is updated when the field modifies the option, so that the options that
/// follow are parsed under the new state.
fn try_parse_short<I: Iterator<Item = Field>>(
    args: &mut Peekable<I>,
    option_occurrences: &mut Vec<(yash_env::option::Option, State)>,
    portable: &mut State,
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
            let attached = chars.as_str();
            // Byte index where the attached option argument starts,
            // or `None` if the option argument is a separate field
            let attached_index = if attached.is_empty() {
                let prev = field;
                field = args.next().ok_or(Error::MissingOptionArgument(prev))?;
                None
            } else {
                Some(field.value.len() - attached.len())
            };
            let raw_name = &field.value[attached_index.unwrap_or(0)..];
            let name = canonicalize(raw_name);
            match parse_long(&name) {
                Ok((option, name_state)) if option.is_modifiable() => {
                    let new_state = if negate { !name_state } else { name_state };
                    if *portable == State::On
                        && !is_portable_long_name(raw_name, option, name_state)
                    {
                        return Err(Error::NonPortableLongOption(
                            field,
                            Some((option, new_state)),
                        ));
                    }
                    if *portable == State::On
                        && let Some(index) = attached_index
                    {
                        // How this field would read with the option argument separated
                        let spelling = format!(
                            "{} {}",
                            quoted(&field.value[..index]),
                            quoted(&field.value[index..])
                        );
                        return Err(Error::UnseparatedOptionArgument(field, spelling));
                    }
                    if option == Portable {
                        *portable = new_state;
                    }
                    option_occurrences.push((option, new_state));
                    break;
                }
                Ok(_) => return Err(Error::UnmodifiableLongOption(field)),
                Err(NoSuchOption) => return Err(Error::UnknownLongOption(field)),
                Err(Ambiguous) => return Err(Error::AmbiguousLongOption(field)),
            }
        }

        match parse_short(c) {
            Some((option, state)) if option.is_modifiable() => {
                if *portable == State::On && option.portable_short_name() != Some((c, state)) {
                    return Err(Error::NonPortableShortOption(c, field));
                }
                option_occurrences.push((option, if negate { !state } else { state }))
            }
            Some(_) => return Err(Error::UnmodifiableShortOption(c, field)),
            None => return Err(Error::UnknownShortOption(c, field)),
        }
    }
    Ok(true)
}

/// Tries to parse and consume the next field in `args`.
///
/// `portable` is the state of the [`Portable`] option in effect for this field.
/// It is updated when the field modifies the option, so that the options that
/// follow are parsed under the new state. POSIX does not define the `--name`
/// and `++name` forms at all, so any of them is rejected while `portable` is
/// on.
fn try_parse_long<I: Iterator<Item = Field>>(
    args: &mut Peekable<I>,
    portable: &mut State,
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
        Ok((option, name_state)) if option.is_modifiable() => {
            let new_state = if negate { !name_state } else { name_state };
            if *portable == State::On {
                return Err(Error::NonPortableLongOption(
                    field,
                    Some((option, new_state)),
                ));
            }
            if option == Portable {
                *portable = new_state;
            }
            Ok(Some((option, new_state)))
        }
        Ok(_) => Err(Error::UnmodifiableLongOption(field)),
        Err(NoSuchOption) => Err(Error::UnknownLongOption(field)),
        Err(Ambiguous) => Err(Error::AmbiguousLongOption(field)),
    }
}

/// Parses command line arguments.
///
/// `portable` is the state of the [`Portable`] shell option when the built-in
/// is invoked. While it is on, the parser accepts only the option syntax POSIX
/// specifies. The arguments are examined in order, so an option that turns
/// `portable` on or off affects only the options that follow it.
pub fn parse(args: Vec<Field>, portable: State) -> Result<Command, Error> {
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
    let mut portable = portable;
    loop {
        if try_parse_short(&mut args, &mut options, &mut portable)? {
            continue;
        }
        if let Some(result) = try_parse_long(&mut args, &mut portable)? {
            options.push(result);
        } else {
            break;
        }
    }

    let separator = args
        .next_if_map(|arg| match arg.value.as_str() {
            "--" => Ok(Ok(arg)),
            "-" if portable == State::Off => Ok(Ok(arg)),
            "-" => Ok(Err(Error::NonPortableSeparator(arg))),
            _ => Err(arg),
        })
        .transpose()?;

    let positional_params = (separator.is_some() || args.peek().is_some()).then(|| args.collect());

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
        assert_eq!(parse(vec![], Off), Ok(Command::PrintVariables));
        assert_eq!(
            parse(Field::dummies(["-o"]), Off),
            Ok(Command::PrintOptionsHumanReadable)
        );
        assert_eq!(
            parse(Field::dummies(["+o"]), Off),
            Ok(Command::PrintOptionsMachineReadable)
        );
    }

    #[test]
    fn positional_params_only() {
        assert_matches!(
            parse(Field::dummies(["foo"]), Off),
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
            parse(Field::dummies([""]), Off),
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
            parse(Field::dummies(["a", "b", "c"]), Off),
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
            parse(Field::dummies(["--"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_eq!(positional_params.unwrap().as_slice(), []);
            }
        );

        assert_matches!(
            parse(Field::dummies(["--", "foo", "bar"]), Off),
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
            parse(Field::dummies(["--", "--"]), Off),
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
            parse(Field::dummies(["--", "-"]), Off),
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
            parse(Field::dummies(["--", "-a"]), Off),
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
            parse(Field::dummies(["-"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_eq!(positional_params.unwrap().as_slice(), []);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-", "foo", "bar"]), Off),
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
            parse(Field::dummies(["-", "-"]), Off),
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
            parse(Field::dummies(["-", "--"]), Off),
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
            parse(Field::dummies(["-", "-a"]), Off),
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
            parse(Field::dummies(["-a"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-uv"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, Off), (Verbose, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-u", "-v"]), Off),
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
            parse(Field::dummies(["+a"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["+uv"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, On), (Verbose, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["+u", "-v"]), Off),
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
            parse(Field::dummies(["-oallexpo"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-o all-Expo"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-onounset"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-o","NO_unset"]), Off),
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
            parse(Field::dummies(["+oallexpo"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["+o all-Expo"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["+onounset"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(Unset, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["+o","NO+unset"]), Off),
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
            parse(Field::dummies(["--allexpo"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["-- all-Expo"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["--nounset"]), Off),
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
            parse(Field::dummies(["++allexpo"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["++ all-Expo"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, Off)]);
                assert_eq!(positional_params, None);
            }
        );

        assert_matches!(
            parse(Field::dummies(["++nounset"]), Off),
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
            parse(Field::dummies(["-a", "--"]), Off),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, [(AllExport, On)]);
                assert_eq!(positional_params, Some(vec![]));
            }
        );

        assert_matches!(
            parse(Field::dummies(["-uv", "--", "-a"]), Off),
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
            parse(Field::dummies(["-n", "-", "--"]), Off),
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
            parse(Field::dummies(["+nononotify", "a", "-a"]), Off),
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
            parse(Field::dummies(["-uno", "-notify", "++log", "--", "foo", "-v"]), Off),
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
            parse(Field::dummies(["-n-a"]), Off),
            Err(Error::UnknownShortOption('-', field)) => {
                assert_eq!(field.value, "-n-a");
            }
        );

        assert_matches!(
            parse(Field::dummies(["--foo"]), Off),
            Err(Error::UnknownLongOption(field)) => {
                assert_eq!(field.value, "--foo");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-ofoo"]), Off),
            Err(Error::UnknownLongOption(field)) => {
                assert_eq!(field.value, "-ofoo");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-o", "foo"]), Off),
            Err(Error::UnknownLongOption(field)) => {
                assert_eq!(field.value, "foo");
            }
        );

        assert_matches!(
            parse(Field::dummies(["--no"]), Off),
            Err(Error::AmbiguousLongOption(field)) => {
                assert_eq!(field.value, "--no");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-oe"]), Off),
            Err(Error::AmbiguousLongOption(field)) => {
                assert_eq!(field.value, "-oe");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-eo"]), Off),
            Err(Error::MissingOptionArgument(field)) => {
                assert_eq!(field.value, "-eo");
            }
        );
    }

    #[test]
    fn unmodifiable_options() {
        assert_matches!(
            parse(Field::dummies(["-c"]), Off),
            Err(Error::UnmodifiableShortOption('c', field)) => {
                assert_eq!(field.value, "-c");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-ointeract"]), Off),
            Err(Error::UnmodifiableLongOption(field)) => {
                assert_eq!(field.value, "-ointeract");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-o", "interact"]), Off),
            Err(Error::UnmodifiableLongOption(field)) => {
                assert_eq!(field.value, "interact");
            }
        );

        assert_matches!(
            parse(Field::dummies(["++stdin"]), Off),
            Err(Error::UnmodifiableLongOption(field)) => {
                assert_eq!(field.value, "++stdin");
            }
        );
    }

    /// Asserts that parsing the arguments with `portable` on yields the
    /// given option occurrences.
    fn assert_portable_options(args: &[&str], expected: &[(yash_env::option::Option, State)]) {
        assert_matches!(
            parse(Field::dummies(args.iter().copied()), On),
            Ok(Command::Modify {
                options,
                positional_params: None,
            }) => assert_eq!(options, expected, "{args:?}"),
            "{args:?}"
        );
    }

    #[test]
    fn portable_accepts_posix_short_options() {
        assert_portable_options(&["-a"], &[(AllExport, On)]);
        assert_portable_options(&["-b"], &[(Notify, On)]);
        assert_portable_options(&["-C"], &[(Clobber, Off)]);
        assert_portable_options(&["-e"], &[(ErrExit, On)]);
        assert_portable_options(&["-f"], &[(Glob, Off)]);
        assert_portable_options(&["-h"], &[(HashOnDefinition, On)]);
        assert_portable_options(&["-m"], &[(Monitor, On)]);
        assert_portable_options(&["-n"], &[(Exec, Off)]);
        assert_portable_options(&["-u"], &[(Unset, Off)]);
        assert_portable_options(&["-v"], &[(Verbose, On)]);
        assert_portable_options(&["-x"], &[(XTrace, On)]);
        assert_portable_options(&["+e"], &[(ErrExit, Off)]);
        assert_portable_options(&["+C"], &[(Clobber, On)]);
        assert_portable_options(&["-ex"], &[(ErrExit, On), (XTrace, On)]);
    }

    #[test]
    fn portable_rejects_short_option_posix_does_not_specify() {
        assert_matches!(
            parse(Field::dummies(["-l"]), On),
            Err(Error::NonPortableShortOption('l', field)) => {
                assert_eq!(field.value, "-l");
            }
        );

        assert_matches!(
            parse(Field::dummies(["+l"]), On),
            Err(Error::NonPortableShortOption('l', field)) => {
                assert_eq!(field.value, "+l");
            }
        );
    }

    #[test]
    fn portable_accepts_posix_o_names() {
        assert_portable_options(&["-o", "allexport"], &[(AllExport, On)]);
        assert_portable_options(&["-o", "errexit"], &[(ErrExit, On)]);
        assert_portable_options(&["-o", "ignoreeof"], &[(IgnoreEof, On)]);
        assert_portable_options(&["-o", "monitor"], &[(Monitor, On)]);
        assert_portable_options(&["-o", "noclobber"], &[(Clobber, Off)]);
        assert_portable_options(&["-o", "noexec"], &[(Exec, Off)]);
        assert_portable_options(&["-o", "noglob"], &[(Glob, Off)]);
        assert_portable_options(&["-o", "nolog"], &[(Log, Off)]);
        assert_portable_options(&["-o", "notify"], &[(Notify, On)]);
        assert_portable_options(&["-o", "nounset"], &[(Unset, Off)]);
        assert_portable_options(&["-o", "pipefail"], &[(PipeFail, On)]);
        assert_portable_options(&["-o", "verbose"], &[(Verbose, On)]);
        assert_portable_options(&["-o", "vi"], &[(Vi, On)]);
        assert_portable_options(&["-o", "xtrace"], &[(XTrace, On)]);
    }

    #[test]
    fn portable_accepts_negated_posix_o_names() {
        assert_portable_options(&["+o", "errexit"], &[(ErrExit, Off)]);
        assert_portable_options(&["+o", "noclobber"], &[(Clobber, On)]);
        assert_portable_options(&["+o", "noexec"], &[(Exec, On)]);
        assert_portable_options(&["+o", "nolog"], &[(Log, On)]);
        assert_portable_options(&["+o", "nounset"], &[(Unset, On)]);
    }

    #[test]
    fn portable_rejects_o_name_posix_spells_negatively() {
        for name in ["clobber", "exec", "glob", "log", "unset"] {
            assert_matches!(
                parse(Field::dummies(["-o", name]), On),
                Err(Error::NonPortableLongOption(field, _)) => {
                    assert_eq!(field.value, name);
                },
                "{name}"
            );
        }
    }

    #[test]
    fn portable_rejects_o_name_that_is_not_spelled_exactly() {
        for name in ["ERREXIT", "err-exit", "errex", "noerrexit"] {
            assert_matches!(
                parse(Field::dummies(["-o", name]), On),
                Err(Error::NonPortableLongOption(field, _)) => {
                    assert_eq!(field.value, name);
                },
                "{name}"
            );
        }
    }

    #[test]
    fn portable_rejects_o_name_posix_does_not_specify() {
        for name in ["hashondefinition", "login", "posixlycorrect"] {
            assert_matches!(
                parse(Field::dummies(["-o", name]), On),
                Err(Error::NonPortableLongOption(field, _)) => {
                    assert_eq!(field.value, name);
                },
                "{name}"
            );
        }
    }

    #[test]
    fn portable_rejects_long_options() {
        for arg in ["--errexit", "++errexit", "--noclobber", "--vi"] {
            assert_matches!(
                parse(Field::dummies([arg]), On),
                Err(Error::NonPortableLongOption(field, _)) => {
                    assert_eq!(field.value, arg);
                },
                "{arg}"
            );
        }
    }

    #[test]
    fn portable_accepts_canonical_spelling_of_portable_option() {
        assert_portable_options(&["-o", "portable"], &[(Portable, On)]);
        assert_portable_options(&["+o", "portable"], &[(Portable, Off)]);
    }

    #[test]
    fn portable_rejects_other_spellings_of_portable_option() {
        for args in [
            ["--portable"].as_slice(),
            ["++portable"].as_slice(),
            ["-o", "PORTABLE"].as_slice(),
            ["-o", "portab"].as_slice(),
            ["-o", "noportable"].as_slice(),
        ] {
            assert_matches!(
                parse(Field::dummies(args.iter().copied()), On),
                Err(Error::NonPortableLongOption(_field, _option)),
                "{args:?}"
            );
        }
    }

    #[test]
    fn enabling_portable_affects_only_following_options() {
        assert_portable_options(
            &["+o", "portable", "--errexit"],
            &[(Portable, Off), (ErrExit, On)],
        );

        assert_matches!(
            parse(Field::dummies(["--errexit", "--portable", "--xtrace"]), Off),
            Err(Error::NonPortableLongOption(field, _)) => {
                assert_eq!(field.value, "--xtrace");
            }
        );

        assert_matches!(
            parse(Field::dummies(["-o", "portable", "-l"]), Off),
            Err(Error::NonPortableShortOption('l', field)) => {
                assert_eq!(field.value, "-l");
            }
        );
    }

    #[test]
    fn disabling_portable_affects_only_following_options() {
        assert_matches!(
            parse(Field::dummies(["-l", "+o", "portable"]), On),
            Err(Error::NonPortableShortOption('l', field)) => {
                assert_eq!(field.value, "-l");
            }
        );
    }

    #[test]
    fn unmodifiable_option_is_reported_rather_than_non_portable() {
        assert_matches!(
            parse(Field::dummies(["-c"]), On),
            Err(Error::UnmodifiableShortOption('c', _field))
        );

        assert_matches!(
            parse(Field::dummies(["-o", "cmdline"]), On),
            Err(Error::UnmodifiableLongOption(_field))
        );
    }

    #[test]
    fn unrecognized_option_name_is_reported_rather_than_non_portable() {
        assert_matches!(
            parse(Field::dummies(["-o", "foo"]), On),
            Err(Error::UnknownLongOption(_field))
        );

        assert_matches!(
            parse(Field::dummies(["-o", "c"]), On),
            Err(Error::AmbiguousLongOption(_field))
        );

        assert_matches!(
            parse(Field::dummies(["-Z"]), On),
            Err(Error::UnknownShortOption('Z', _field))
        );
    }

    #[test]
    fn non_portable_error_reports_the_reason() {
        let error = parse(Field::dummies(["-l"]), On).unwrap_err();
        let report = error.to_report();
        assert_eq!(report.title, "non-portable option 'l'");
        assert_eq!(report.footnotes.len(), 1);
        assert_eq!(
            report.footnotes[0].label,
            "this error is reported because the `portable` shell option is enabled"
        );
    }

    #[test]
    fn non_portable_error_suggests_portable_spelling() {
        fn suggestion(args: &[&str]) -> String {
            let error = parse(Field::dummies(args.iter().copied()), On).unwrap_err();
            let footnotes = error.to_report().footnotes;
            assert_eq!(footnotes.len(), 2, "{args:?}");
            footnotes[1].label.to_string()
        }

        assert_eq!(suggestion(&["--errexit"]), "use `-o errexit` instead");
        assert_eq!(suggestion(&["++errexit"]), "use `+o errexit` instead");
        assert_eq!(suggestion(&["-o", "clobber"]), "use `+o noclobber` instead");
        assert_eq!(suggestion(&["-o", "noerrexit"]), "use `+o errexit` instead");
        assert_eq!(suggestion(&["-o", "ERREXIT"]), "use `-o errexit` instead");
        assert_eq!(suggestion(&["-o", "hashondefinition"]), "use `-h` instead");
        assert_eq!(suggestion(&["--portable"]), "use `-o portable` instead");
        assert_eq!(suggestion(&["++portable"]), "use `+o portable` instead");
    }

    #[test]
    fn portable_rejects_option_argument_attached_to_the_option_name() {
        for (args, spelling) in [
            (["-oerrexit"], "-o errexit"),
            (["-aoerrexit"], "-ao errexit"),
            (["+onoclobber"], "+o noclobber"),
            (["-oportable"], "-o portable"),
        ] {
            assert_matches!(
                parse(Field::dummies(args), On),
                Err(Error::UnseparatedOptionArgument(field, suggested)) => {
                    assert_eq!(field.value, args[0]);
                    assert_eq!(suggested, spelling);
                },
                "{args:?}"
            );
        }
    }

    #[test]
    fn non_portable_option_name_is_reported_before_the_attached_argument() {
        assert_matches!(
            parse(Field::dummies(["-oclobber"]), On),
            Err(Error::NonPortableLongOption(_field, _option))
        );
    }

    #[test]
    fn unseparated_option_argument_error_reports_the_reason_and_spelling() {
        let error = parse(Field::dummies(["-oerrexit"]), On).unwrap_err();
        let footnotes = error.to_report().footnotes;
        assert_eq!(footnotes.len(), 2);
        assert_eq!(
            footnotes[0].label,
            "this error is reported because the `portable` shell option is enabled"
        );
        assert_eq!(footnotes[1].label, "use `-o errexit` instead");
    }

    #[test]
    fn non_portable_error_omits_suggestion_when_posix_has_no_name() {
        let error = parse(Field::dummies(["--login"]), On).unwrap_err();
        assert_eq!(error.to_report().footnotes.len(), 1);
    }

    #[test]
    fn portable_rejects_single_hyphen_separator() {
        for args in [
            ["-"].as_slice(),
            ["-", "foo"].as_slice(),
            ["-", "-"].as_slice(),
            ["-a", "-"].as_slice(),
            ["-a", "-", "foo"].as_slice(),
        ] {
            assert_matches!(
                parse(Field::dummies(args.iter().copied()), On),
                Err(Error::NonPortableSeparator(field)) => {
                    assert_eq!(field.value, "-");
                },
                "{args:?}"
            );
        }
    }

    #[test]
    fn portable_accepts_double_hyphen_separator() {
        assert_matches!(
            parse(Field::dummies(["--"]), On),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_eq!(positional_params.unwrap().as_slice(), []);
            }
        );

        assert_matches!(
            parse(Field::dummies(["--", "-", "foo"]), On),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first, second] => {
                    assert_eq!(first.value, "-");
                    assert_eq!(second.value, "foo");
                });
            }
        );
    }

    #[test]
    fn portable_accepts_single_hyphen_that_is_not_a_separator() {
        assert_matches!(
            parse(Field::dummies(["foo", "-"]), On),
            Ok(Command::Modify {
                options,
                positional_params
            }) => {
                assert_eq!(options, []);
                assert_matches!(positional_params.unwrap().as_slice(), [first, second] => {
                    assert_eq!(first.value, "foo");
                    assert_eq!(second.value, "-");
                });
            }
        );
    }

    #[test]
    fn portable_state_at_the_separator_decides_the_rejection() {
        assert_matches!(
            parse(Field::dummies(["+o", "portable", "-", "foo"]), On),
            Ok(Command::Modify { .. })
        );

        assert_matches!(
            parse(Field::dummies(["-o", "portable", "-", "foo"]), Off),
            Err(Error::NonPortableSeparator(field)) => {
                assert_eq!(field.value, "-");
            }
        );
    }

    #[test]
    fn non_portable_separator_error_reports_the_reason_and_spelling() {
        let error = parse(Field::dummies(["-", "foo"]), On).unwrap_err();
        let report = error.to_report();
        assert_eq!(report.title, "non-portable option-operand separator \"-\"");
        assert_eq!(report.footnotes.len(), 2);
        assert_eq!(
            report.footnotes[0].label,
            "this error is reported because the `portable` shell option is enabled"
        );
        assert_eq!(report.footnotes[1].label, "use `--` instead");
    }
}
