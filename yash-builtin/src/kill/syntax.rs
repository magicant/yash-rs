// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Command line parsing
//!
//! This module parses command line arguments to the kill built-in.
//! The parser is implemented without using the utilities in the
//! [`crate::common::syntax`] crate because of the special syntax of the
//! signal-specifying option.

use super::Command;
use std::borrow::Cow;
use thiserror::Error;
use yash_env::Env;
use yash_env::option::{Portable, State::On};
use yash_env::semantics::Field;
use yash_env::signal::{Number, RawNumber};
use yash_env::source::Location;
use yash_env::source::pretty::{
    Footnote, FootnoteType, Report, ReportType, Snippet, Span, SpanRole, add_span,
};
use yash_env::system::Signals;
use yash_quote::quoted;

/// Error that may occur during parsing
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// An argument starts with a hyphen (`-`) but is not a valid option.
    #[error("unknown option")]
    UnknownOption(Field),

    /// An option that POSIX does not define is used.
    ///
    /// This error occurs only when the [`Portable`] shell option is on. The
    /// options in question are `-n` and `-v`. The `char` is the name of the
    /// offending option, and the `Field` is the argument containing it.
    #[error("non-portable option {0:?}")]
    NonPortableOption(char, Field),

    /// The signal to send is specified and the `-l` or `-v` option is also
    /// specified.
    #[error("invalid option combination")]
    ConflictingOptions {
        /// Command line argument that specifies the signal to send
        signal_arg: Field,
        /// Name of the option that requests a list (`l` or `v`)
        list_option_name: char,
        /// Location of the `-l` or `-v` option
        list_option_location: Location,
    },

    /// The `-s` or `-n` option is not followed by a signal name or number.
    #[error("missing signal name or number")]
    MissingSignal {
        /// Name of the option for specifying a signal (`s` or `n`)
        signal_option_name: char,
        /// Location of the `-s` or `-n` option
        signal_option_location: Location,
    },

    /// The argument to the `-s` option is in the same field as the option.
    ///
    /// This error occurs only when the [`Portable`] shell option is on. POSIX
    /// requires an option argument to be a separate argument, so `-s INT` must
    /// be used instead of `-sINT`.
    #[error("option argument not separated from the option name in {:?}", .field.value)]
    UnseparatedSignalArgument {
        /// Field containing the option and its argument
        field: Field,
        /// Byte index of the argument in the field value
        argument_index: usize,
    },

    /// A signal number other than `0` is specified as the argument to the `-s`
    /// option.
    ///
    /// This error occurs only when the [`Portable`] shell option is on. POSIX
    /// requires the argument to the `-s` option to be a signal name or `0`.
    #[error("non-portable signal number {number}")]
    NonPortableSignalNumber {
        /// Field containing the signal number
        field: Field,
        /// Signal number specified in the field
        number: RawNumber,
    },

    /// A signal name with the `SIG` prefix is specified.
    ///
    /// This error occurs only when the [`Portable`] shell option is on. POSIX
    /// requires a signal name to be specified without the `SIG` prefix, so
    /// `-s INT` must be used instead of `-s SIGINT`.
    #[error("non-portable signal name {:?}", &.field.value[*.name_index..])]
    NonPortableSignalPrefix {
        /// Field containing the signal name
        field: Field,
        /// Byte index of the signal name in the field value
        ///
        /// The signal name may be preceded by the option name as in `-sSIGINT`
        /// or by a single hyphen as in `-SIGINT`, in which case this index
        /// points to the signal name after them.
        name_index: usize,
    },

    /// More than one signal to send is specified.
    #[error("multiple signals specified")]
    MultipleSignals(Field, Field),

    /// A specified signal is not a valid signal name or number.
    ///
    /// This error is returned when the argument to the `-s` or `-n` option is
    /// not a valid signal name or number. This error also occurs when an
    /// operand given with the `-l` or `-v` option is not a valid signal name,
    /// signal number, or exit status.
    #[error("invalid signal")]
    InvalidSignal(Field),

    /// More than one operand is specified with the `-l` option.
    ///
    /// This error occurs only when the [`Portable`] shell option is on. POSIX
    /// allows at most one operand for the `-l` option.
    #[error("multiple operands specified")]
    MultipleListOperands(Field, Field),

    /// A signal name is specified as an operand to the `-l` option.
    ///
    /// This error occurs only when the [`Portable`] shell option is on. POSIX
    /// requires the operand for the `-l` option to be an exit status or a
    /// signal number.
    #[error("non-portable operand {:?}", .0.value)]
    NonPortableListOperand(Field),

    /// No target is specified and the `-l` or `-v` option is not specified.
    #[error("no target process specified")]
    MissingTarget,
}

impl Error {
    /// Converts this error to a report
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();
        report.snippets = match self {
            Self::UnknownOption(field) => Snippet::with_primary_span(
                &field.origin,
                format!("{:?} is not a valid option", field.value).into(),
            ),

            Self::NonPortableOption(option, field) => Snippet::with_primary_span(
                &field.origin,
                format!("option `{option}` is not defined in POSIX").into(),
            ),

            Self::ConflictingOptions {
                signal_arg,
                list_option_name,
                list_option_location,
            } => {
                let mut snippets = Snippet::with_primary_span(
                    &signal_arg.origin,
                    "signal to send is specified here".into(),
                );
                add_span(
                    &list_option_location.code,
                    Span {
                        range: list_option_location.byte_range(),
                        role: SpanRole::Primary {
                            label: format!("option `{list_option_name}` is incompatible").into(),
                        },
                    },
                    &mut snippets,
                );
                snippets
            }

            Self::MissingSignal {
                signal_option_name,
                signal_option_location,
            } => Snippet::with_primary_span(
                signal_option_location,
                format!("option `{signal_option_name}` requires a signal name or number").into(),
            ),

            Self::UnseparatedSignalArgument { field, .. } => Snippet::with_primary_span(
                &field.origin,
                "the option argument must be a separate argument".into(),
            ),

            Self::NonPortableSignalNumber { field, .. } => Snippet::with_primary_span(
                &field.origin,
                "POSIX requires a signal name or `0` here".into(),
            ),

            Self::NonPortableSignalPrefix { field, .. } => Snippet::with_primary_span(
                &field.origin,
                "POSIX does not allow the `SIG` prefix in a signal name".into(),
            ),

            Self::MultipleSignals(field1, field2) => {
                let mut snippets = Snippet::with_primary_span(
                    &field1.origin,
                    format!("first signal {:?}", field1.value).into(),
                );
                add_span(
                    &field2.origin.code,
                    Span {
                        range: field2.origin.byte_range(),
                        role: SpanRole::Primary {
                            label: format!("second signal {:?}", field2.value).into(),
                        },
                    },
                    &mut snippets,
                );
                snippets
            }

            Self::InvalidSignal(field) => Snippet::with_primary_span(
                &field.origin,
                format!("{:?} is not a valid signal name or number", field.value).into(),
            ),

            Self::MultipleListOperands(field1, field2) => {
                let mut snippets = Snippet::with_primary_span(
                    &field1.origin,
                    format!("first operand {:?}", field1.value).into(),
                );
                add_span(
                    &field2.origin.code,
                    Span {
                        range: field2.origin.byte_range(),
                        role: SpanRole::Primary {
                            label: format!("second operand {:?}", field2.value).into(),
                        },
                    },
                    &mut snippets,
                );
                snippets
            }

            Self::NonPortableListOperand(field) => Snippet::with_primary_span(
                &field.origin,
                "POSIX requires an exit status or signal number here".into(),
            ),

            Self::MissingTarget => vec![],
        };

        if let Self::NonPortableOption(..)
        | Self::UnseparatedSignalArgument { .. }
        | Self::NonPortableSignalNumber { .. }
        | Self::NonPortableSignalPrefix { .. }
        | Self::MultipleListOperands(..)
        | Self::NonPortableListOperand(..) = self
        {
            report.footnotes.push(Footnote {
                r#type: FootnoteType::Note,
                label: "this error is reported because the `portable` shell option is enabled"
                    .into(),
            });
        }

        match self {
            Self::UnseparatedSignalArgument {
                field,
                argument_index,
            } => report.footnotes.push(Footnote {
                r#type: FootnoteType::Suggestion,
                label: format!(
                    "use `{} {}` instead",
                    quoted(&field.value[..*argument_index]),
                    quoted(&field.value[*argument_index..]),
                )
                .into(),
            }),

            // A negative number would yield a suggestion like `--1`, which does
            // not specify a signal at all, so no suggestion is made for it.
            Self::NonPortableSignalNumber { number, .. } if *number > 0 => {
                report.footnotes.push(Footnote {
                    r#type: FootnoteType::Suggestion,
                    label: format!("use `-{number}` instead").into(),
                })
            }

            Self::NonPortableSignalPrefix { field, name_index } => {
                // The `-s NAME` form is portable wherever the offending name
                // appeared, so it is suggested regardless of the form used. It
                // also resolves the attachment of `-sSIGINT`, which is not
                // portable either.
                let name = portable_signal_name(&field.value[*name_index..]);
                report.footnotes.push(Footnote {
                    r#type: FootnoteType::Suggestion,
                    label: format!("use `-s {}` instead", quoted(&name)).into(),
                })
            }

            _ => {}
        }

        report
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

/// Converts a string to a signal.
///
/// The string may be a signal name or a number.
///
/// If the string is a valid signal name as per [`Signals::str2sig`], this
/// function returns the corresponding signal number. If the string is a decimal
/// integer, this function returns its value as a signal number regardless of
/// whether it corresponds to a valid signal. Otherwise, this function returns
/// `None`.
///
/// The signal name is parsed case-insensitively.
///
/// If `allow_sig_prefix` is `true`, the `SIG` prefix is optional for signal
/// names. Otherwise, the `SIG` prefix must **not** be present.
#[must_use]
pub fn parse_signal<S: Signals>(
    system: &S,
    signal_spec: &str,
    allow_sig_prefix: bool,
) -> Option<RawNumber> {
    // Try parsing as a number first
    if let Ok(number) = signal_spec.parse() {
        return Some(number);
    }

    // Make the string uppercase for case-insensitive comparison
    let mut signal_spec = Cow::Borrowed(signal_spec);
    if signal_spec.contains(|c: char| c.is_ascii_lowercase()) {
        signal_spec.to_mut().make_ascii_uppercase();
    }

    // Remove the SIG prefix if allowed
    let signal_name = allow_sig_prefix
        .then(|| signal_spec.strip_prefix("SIG"))
        .flatten()
        .unwrap_or(&signal_spec);

    // Parse as a signal name
    system.str2sig(signal_name).map(Number::as_raw)
}

/// Tests whether a signal specification is a signal name rather than a number.
///
/// This function returns `true` if the string is not a decimal integer but is a
/// valid signal name as per [`parse_signal`].
#[must_use]
fn is_signal_name<S: Signals>(system: &S, signal_spec: &str, allow_sig_prefix: bool) -> bool {
    signal_spec.parse::<RawNumber>().is_err()
        && parse_signal(system, signal_spec, allow_sig_prefix).is_some()
}

/// Returns the signal number if the string specifies a signal number that POSIX
/// does not allow as the argument to the `-s` option.
///
/// POSIX requires the argument to be a signal name or `0`, so any other number
/// is non-portable.
#[must_use]
fn non_portable_signal_number(signal_spec: &str) -> Option<RawNumber> {
    match signal_spec.parse() {
        Ok(0) | Err(_) => None,
        Ok(number) => Some(number),
    }
}

/// Returns the portable form of a signal name that has the `SIG` prefix.
///
/// The name is uppercased and the `SIG` prefix is removed.
#[must_use]
fn portable_signal_name(signal_spec: &str) -> String {
    let mut uppercase = signal_spec.to_ascii_uppercase();
    if uppercase.starts_with("SIG") {
        uppercase.drain(..3);
    }
    uppercase
}

/// Checks that a signal specification does not have the non-portable `SIG`
/// prefix.
///
/// `name_index` is the byte index at which the signal specification starts in
/// `field.value`. This function returns [`Error::NonPortableSignalPrefix`] if
/// the specification is a valid signal name only when the `SIG` prefix is
/// allowed.
fn check_portable_signal_prefix<S: Signals>(
    system: &S,
    field: &Field,
    name_index: usize,
) -> Result<(), Error> {
    let signal_spec = &field.value[name_index..];
    if parse_signal(system, signal_spec, false).is_none()
        && parse_signal(system, signal_spec, true).is_some()
    {
        return Err(Error::NonPortableSignalPrefix {
            field: field.clone(),
            name_index,
        });
    }
    Ok(())
}

/// Checks that the operands to the `-l` or `-v` option are portable.
///
/// POSIX allows at most one operand, which must be an exit status or a signal
/// number.
fn check_portable_list_operands<S: Signals>(
    system: &S,
    operands: &[Field],
    allow_sig_prefix: bool,
) -> Result<(), Error> {
    if let Some(first) = operands.first()
        && is_signal_name(system, &first.value, allow_sig_prefix)
    {
        return Err(Error::NonPortableListOperand(first.clone()));
    }
    if let [first, second, ..] = operands {
        return Err(Error::MultipleListOperands(first.clone(), second.clone()));
    }
    Ok(())
}

/// Updates a signal and its origin.
///
/// `new_signal` is the new value of `signal`. It should be the result of
/// [`parse_signal`]. If it is `None`, this function returns
/// `Error::InvalidSignal(new_signal_origin)`.
///
/// `new_signal_origin` should be the field containing the string that was
/// parsed to obtain `new_signal`. It is used to update `signal_origin`.
/// However, if `signal_origin` already contains a field, this function returns
/// `Error::MultipleSignals(signal_origin.take().unwrap(), new_signal_origin)`.
fn set_signal(
    signal: &mut RawNumber,
    signal_origin: &mut Option<Field>,
    new_signal: Option<RawNumber>,
    new_signal_origin: Field,
) -> Result<(), Error> {
    let Some(new_signal) = new_signal else {
        return Err(Error::InvalidSignal(new_signal_origin));
    };
    if let Some(prev) = signal_origin.take() {
        return Err(Error::MultipleSignals(prev, new_signal_origin));
    }
    *signal = new_signal;
    *signal_origin = Some(new_signal_origin);
    Ok(())
}

/// Converts an invalid signal error to an unknown option error.
#[must_use]
fn invalid_signal_to_unknown_option(error: Error) -> Error {
    match error {
        Error::InvalidSignal(field) => Error::UnknownOption(field),
        error => error,
    }
}

/// Parses operands after the `-l` or `-v` option, returning the final command.
fn parse_list_case<I: Iterator<Item = Field>>(
    operands: I,
    signal_origin: Option<Field>,
    list_option_name: char,
    list_option_location: Location,
    verbose: bool,
) -> Result<Command, Error> {
    if let Some(signal_arg) = signal_origin {
        Err(Error::ConflictingOptions {
            signal_arg,
            list_option_name,
            list_option_location,
        })
    } else {
        let signals = operands.collect();
        Ok(Command::Print { signals, verbose })
    }
}

/// Parses command line arguments.
pub fn parse<S: Signals>(env: &Env<S>, args: Vec<Field>) -> Result<Command, Error> {
    let portable = env.options.get(Portable) == On;
    // POSIX requires the signal name to be specified without the SIG prefix.
    let allow_sig_prefix = !portable;
    let mut args = args.into_iter().peekable();
    let mut signal = S::SIGTERM.as_raw();
    let mut signal_origin = None;
    let mut list = None;
    let mut verbose = None;

    // Parse options
    while let Some(arg) =
        args.next_if(|arg| arg.value.strip_prefix('-').is_some_and(|s| !s.is_empty()))
    {
        let options = &arg.value[1..];
        if options == "-" {
            debug_assert_eq!(arg.value, "--");
            break;
        }

        let mut chars = options.chars();
        while let Some(option) = chars.next() {
            match option {
                // POSIX defines neither the `-n` nor the `-v` option.
                'n' | 'v' if portable => return Err(Error::NonPortableOption(option, arg)),

                's' | 'n' => {
                    let remainder = chars.as_str();
                    if remainder.is_empty() {
                        let Some(current_signal_arg) = args.next() else {
                            return Err(Error::MissingSignal {
                                signal_option_name: option,
                                signal_option_location: arg.origin,
                            });
                        };
                        if portable {
                            if let Some(number) =
                                non_portable_signal_number(&current_signal_arg.value)
                            {
                                return Err(Error::NonPortableSignalNumber {
                                    field: current_signal_arg,
                                    number,
                                });
                            }
                            check_portable_signal_prefix(&env.system, &current_signal_arg, 0)?;
                        }
                        set_signal(
                            &mut signal,
                            &mut signal_origin,
                            parse_signal(&env.system, &current_signal_arg.value, allow_sig_prefix),
                            current_signal_arg,
                        )?;
                    } else {
                        if portable {
                            // The form of the argument is examined before its
                            // attachment so that the suggested replacement is
                            // itself portable.
                            if let Some(number) = non_portable_signal_number(remainder) {
                                return Err(Error::NonPortableSignalNumber { field: arg, number });
                            }
                            // If the remainder is a valid signal specification,
                            // it is an option argument attached to the option
                            // name. Otherwise, the whole cluster may still be a
                            // signal name as in `-stop`, which POSIX allows.
                            if parse_signal(&env.system, remainder, allow_sig_prefix).is_some() {
                                let argument_index = arg.value.len() - remainder.len();
                                return Err(Error::UnseparatedSignalArgument {
                                    field: arg,
                                    argument_index,
                                });
                            }
                            // The remainder may be a signal name that is valid
                            // only with the `SIG` prefix, as in `-sSIGINT`. If
                            // it is not, the whole cluster may still be such a
                            // name, as in `-sigstop`.
                            let name_index = arg.value.len() - remainder.len();
                            check_portable_signal_prefix(&env.system, &arg, name_index)?;
                            check_portable_signal_prefix(&env.system, &arg, 1)?;
                        }
                        set_signal(
                            &mut signal,
                            &mut signal_origin,
                            parse_signal(&env.system, remainder, allow_sig_prefix)
                                .or_else(|| parse_signal(&env.system, options, allow_sig_prefix)),
                            arg,
                        )?;
                    }
                    break;
                }
                'l' => {
                    list = Some(arg.origin.clone());
                }
                'v' => {
                    verbose = Some(arg.origin.clone());
                }
                _ => {
                    if portable {
                        // Without this check, a signal name that is valid only
                        // with the `SIG` prefix would be reported as an unknown
                        // option, which would not explain the real cause.
                        check_portable_signal_prefix(&env.system, &arg, 1)?;
                    }
                    set_signal(
                        &mut signal,
                        &mut signal_origin,
                        parse_signal(&env.system, options, allow_sig_prefix),
                        arg,
                    )
                    .map_err(invalid_signal_to_unknown_option)?;
                    break;
                }
            }
        }
    }

    // Parse operands and compute the result
    let command = if let Some(option_location) = verbose {
        parse_list_case(args, signal_origin, 'v', option_location, true)
    } else if let Some(option_location) = list {
        parse_list_case(args, signal_origin, 'l', option_location, false)
    } else {
        // Command::Send case
        if args.peek().is_none() {
            Err(Error::MissingTarget)
        } else {
            let targets = args.collect();
            Ok(Command::Send {
                signal,
                signal_origin,
                targets,
            })
        }
    }?;

    if portable && let Command::Print { signals, .. } = &command {
        check_portable_list_operands(&env.system, signals, allow_sig_prefix)?;
    }

    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::system::r#virtual::VirtualSystem;

    #[test]
    fn parse_signal_names_without_sig_prefix() {
        let system = VirtualSystem::new();
        assert_eq!(
            parse_signal(&system, "INT", false),
            Some(VirtualSystem::SIGINT.as_raw())
        );
        assert_eq!(
            parse_signal(&system, "RtMin+5", false),
            Some(system.sigrt_range().unwrap().start().as_raw() + 5)
        );
        assert_eq!(parse_signal(&system, "SigRtMin+5", false), None);
    }

    #[test]
    fn parse_signal_names_with_sig_prefix() {
        let system = VirtualSystem::new();
        assert_eq!(
            parse_signal(&system, "INT", true),
            Some(VirtualSystem::SIGINT.as_raw())
        );
        assert_eq!(
            parse_signal(&system, "RtMin+5", true),
            Some(system.sigrt_range().unwrap().start().as_raw() + 5)
        );
        assert_eq!(
            parse_signal(&system, "SigRtMin+5", true),
            Some(system.sigrt_range().unwrap().start().as_raw() + 5)
        );
    }

    #[test]
    fn parse_signal_numbers() {
        let system = VirtualSystem::new();
        assert_eq!(parse_signal(&system, "0", false), Some(0));
        assert_eq!(parse_signal(&system, "1", false), Some(1));
        assert_eq!(parse_signal(&system, "3", true), Some(3));
        assert_eq!(parse_signal(&system, "6", false), Some(6));
        assert_eq!(parse_signal(&system, "9", true), Some(9));
        assert_eq!(parse_signal(&system, "14", true), Some(14));
    }

    #[test]
    fn parse_signal_errors() {
        let system = VirtualSystem::new();
        assert_eq!(parse_signal(&system, "", false), None);
        assert_eq!(parse_signal(&system, "TERM1", false), None);
        assert_eq!(parse_signal(&system, "1TERM", false), None);
    }

    #[test]
    fn empty_operand() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies([""]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGTERM.as_raw(),
                signal_origin: None,
                targets: Field::dummies([""]),
            })
        )
    }

    #[test]
    fn single_hyphen_operand() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGTERM.as_raw(),
                signal_origin: None,
                targets: Field::dummies(["-"]),
            })
        );
    }

    #[test]
    fn double_hyphen_separator() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-s", "INT", "--", "0"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGINT.as_raw(),
                signal_origin: Some(Field::dummy("INT")),
                targets: Field::dummies(["0"]),
            })
        );

        let result = parse(&env, Field::dummies(["-l", "--", "9"]));
        assert_eq!(
            result,
            Ok(Command::Print {
                signals: Field::dummies(["9"]),
                verbose: false,
            })
        );
    }

    #[test]
    fn option_s_with_separate_signal_name_argument() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-s", "QuIt", "1"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGQUIT.as_raw(),
                signal_origin: Some(Field::dummy("QuIt")),
                targets: Field::dummies(["1"]),
            })
        );
    }

    #[test]
    fn option_s_with_adjacent_signal_name_argument() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-sQuIt", "1"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGQUIT.as_raw(),
                signal_origin: Some(Field::dummy("-sQuIt")),
                targets: Field::dummies(["1"]),
            })
        );
    }

    #[test]
    fn option_s_with_separate_signal_number_argument() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-s", "9", "1"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: 9,
                signal_origin: Some(Field::dummy("9")),
                targets: Field::dummies(["1"]),
            })
        );
    }

    #[test]
    fn option_n_with_separate_signal_name_argument() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-n", "QuIt", "1"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGQUIT.as_raw(),
                signal_origin: Some(Field::dummy("QuIt")),
                targets: Field::dummies(["1"]),
            })
        );
    }

    #[test]
    fn bare_signal_name_in_uppercase() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-KILL", "1"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGKILL.as_raw(),
                signal_origin: Some(Field::dummy("-KILL")),
                targets: Field::dummies(["1"]),
            })
        );
    }

    #[test]
    fn bare_signal_name_starting_with_s() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-stop", "1"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGSTOP.as_raw(),
                signal_origin: Some(Field::dummy("-stop")),
                targets: Field::dummies(["1"]),
            })
        );
    }

    #[test]
    fn base_signal_number() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-9", "1"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: 9,
                signal_origin: Some(Field::dummy("-9")),
                targets: Field::dummies(["1"]),
            })
        );
    }

    #[test]
    fn option_l_without_operands() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-l"]));
        assert_eq!(
            result,
            Ok(Command::Print {
                signals: vec![],
                verbose: false,
            })
        );
    }

    #[test]
    fn option_v_without_operands() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-v"]));
        assert_eq!(
            result,
            Ok(Command::Print {
                signals: vec![],
                verbose: true,
            })
        );
    }

    #[test]
    fn option_l_and_v_combined() {
        let env = Env::new_virtual();
        let expected_result = Ok(Command::Print {
            signals: vec![],
            verbose: true,
        });

        assert_eq!(parse(&env, Field::dummies(["-lv"])), expected_result);
        assert_eq!(parse(&env, Field::dummies(["-vl"])), expected_result);
        assert_eq!(parse(&env, Field::dummies(["-l", "-v"])), expected_result);
        assert_eq!(parse(&env, Field::dummies(["-v", "-l"])), expected_result);
    }

    #[test]
    fn option_l_with_operands() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-l", "Term", "1"]));
        assert_eq!(
            result,
            Ok(Command::Print {
                signals: Field::dummies(["Term", "1"]),
                verbose: false,
            })
        );
    }

    #[test]
    fn unknown_option() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-x"]));
        assert_eq!(result, Err(Error::UnknownOption(Field::dummy("-x"))));
    }

    #[test]
    fn option_s_conflicts_with_option_l() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-s", "TERM", "-l"]));
        assert_eq!(
            result,
            Err(Error::ConflictingOptions {
                signal_arg: Field::dummy("TERM"),
                list_option_name: 'l',
                list_option_location: Location::dummy("-l"),
            })
        );

        let result = parse(&env, Field::dummies(["-ls", "TERM"]));
        assert_eq!(
            result,
            Err(Error::ConflictingOptions {
                signal_arg: Field::dummy("TERM"),
                list_option_name: 'l',
                list_option_location: Location::dummy("-ls"),
            })
        );
    }

    #[test]
    fn option_n_conflicts_with_option_l() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-n", "9", "-l"]));
        assert_eq!(
            result,
            Err(Error::ConflictingOptions {
                signal_arg: Field::dummy("9"),
                list_option_name: 'l',
                list_option_location: Location::dummy("-l"),
            })
        );

        let result = parse(&env, Field::dummies(["-ln", "9"]));
        assert_eq!(
            result,
            Err(Error::ConflictingOptions {
                signal_arg: Field::dummy("9"),
                list_option_name: 'l',
                list_option_location: Location::dummy("-ln"),
            })
        );
    }

    #[test]
    fn option_s_conflicts_with_option_v() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-s", "TERM", "-v"]));
        assert_eq!(
            result,
            Err(Error::ConflictingOptions {
                signal_arg: Field::dummy("TERM"),
                list_option_name: 'v',
                list_option_location: Location::dummy("-v"),
            })
        );

        let result = parse(&env, Field::dummies(["-lvls", "TERM"]));
        assert_eq!(
            result,
            Err(Error::ConflictingOptions {
                signal_arg: Field::dummy("TERM"),
                list_option_name: 'v',
                list_option_location: Location::dummy("-lvls"),
            })
        );
    }

    #[test]
    fn option_n_conflicts_with_option_v() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-n", "9", "-v"]));
        assert_eq!(
            result,
            Err(Error::ConflictingOptions {
                signal_arg: Field::dummy("9"),
                list_option_name: 'v',
                list_option_location: Location::dummy("-v"),
            })
        );

        let result = parse(&env, Field::dummies(["-lvln", "9"]));
        assert_eq!(
            result,
            Err(Error::ConflictingOptions {
                signal_arg: Field::dummy("9"),
                list_option_name: 'v',
                list_option_location: Location::dummy("-lvln"),
            })
        );
    }

    #[test]
    fn option_s_without_signal() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-s"]));
        assert_eq!(
            result,
            Err(Error::MissingSignal {
                signal_option_name: 's',
                signal_option_location: Location::dummy("-s"),
            })
        );
    }

    #[test]
    fn option_n_without_signal() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-n"]));
        assert_eq!(
            result,
            Err(Error::MissingSignal {
                signal_option_name: 'n',
                signal_option_location: Location::dummy("-n"),
            })
        );
    }

    #[test]
    fn multiple_signals_error_on_option_s() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-INT", "-s", "TERM"]));
        assert_eq!(
            result,
            Err(Error::MultipleSignals(
                Field::dummy("-INT"),
                Field::dummy("TERM")
            ))
        );
    }

    #[test]
    fn multiple_signals_error_on_option_n() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-s", "TERM", "-nINT"]));
        assert_eq!(
            result,
            Err(Error::MultipleSignals(
                Field::dummy("TERM"),
                Field::dummy("-nINT")
            ))
        );
    }

    #[test]
    fn multiple_signals_error_on_bare_signal_name() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-n", "TERM", "-QUIT"]));
        assert_eq!(
            result,
            Err(Error::MultipleSignals(
                Field::dummy("TERM"),
                Field::dummy("-QUIT")
            ))
        );
    }

    #[test]
    fn invalid_separate_signal_argument_to_option_s() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-s", "TERM1", "123"]));
        assert_eq!(result, Err(Error::InvalidSignal(Field::dummy("TERM1"))));
    }

    #[test]
    fn invalid_separate_signal_argument_to_option_n() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-n", "TERM1", "123"]));
        assert_eq!(result, Err(Error::InvalidSignal(Field::dummy("TERM1"))));
    }

    #[test]
    fn invalid_adjoined_signal_argument_to_option_s() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-sTERM1", "123"]));
        assert_eq!(result, Err(Error::InvalidSignal(Field::dummy("-sTERM1"))));
    }

    #[test]
    fn missing_target() {
        let env = Env::new_virtual();
        let result = parse(&env, vec![]);
        assert_eq!(result, Err(Error::MissingTarget));
    }

    fn portable_env() -> Env<impl Signals> {
        let mut env = Env::new_virtual();
        env.options.set(Portable, On);
        env
    }

    #[test]
    fn option_n_rejected_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-n", "TERM", "123"]));
        assert_eq!(
            result,
            Err(Error::NonPortableOption('n', Field::dummy("-n")))
        );
    }

    #[test]
    fn option_v_rejected_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-v"]));
        assert_eq!(
            result,
            Err(Error::NonPortableOption('v', Field::dummy("-v")))
        );
    }

    #[test]
    fn non_portable_option_report() {
        let error = Error::NonPortableOption('n', Field::dummy("-n"));
        assert_eq!(error.to_string(), "non-portable option 'n'");
        let report = error.to_report();
        assert_eq!(report.footnotes.len(), 1);
        assert_eq!(
            report.footnotes[0].label,
            "this error is reported because the `portable` shell option is enabled"
        );
    }

    #[test]
    fn option_l_still_accepted_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-l"]));
        assert_eq!(
            result,
            Ok(Command::Print {
                signals: vec![],
                verbose: false,
            })
        );
    }

    #[test]
    fn unseparated_signal_argument_rejected_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-sINT", "123"]));
        assert_eq!(
            result,
            Err(Error::UnseparatedSignalArgument {
                field: Field::dummy("-sINT"),
                argument_index: 2,
            })
        );
    }

    #[test]
    fn unseparated_signal_argument_report() {
        let error = Error::UnseparatedSignalArgument {
            field: Field::dummy("-sINT"),
            argument_index: 2,
        };
        assert_eq!(
            error.to_string(),
            "option argument not separated from the option name in \"-sINT\""
        );
        let report = error.to_report();
        assert_eq!(report.footnotes.len(), 2);
        assert_eq!(
            report.footnotes[0].label,
            "this error is reported because the `portable` shell option is enabled"
        );
        assert_eq!(report.footnotes[1].label, "use `-s INT` instead");
    }

    #[test]
    fn separate_signal_argument_accepted_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-s", "INT", "123"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGINT.as_raw(),
                signal_origin: Some(Field::dummy("INT")),
                targets: Field::dummies(["123"]),
            })
        );
    }

    #[test]
    fn bare_signal_name_starting_with_s_accepted_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-stop", "123"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGSTOP.as_raw(),
                signal_origin: Some(Field::dummy("-stop")),
                targets: Field::dummies(["123"]),
            })
        );
    }

    #[test]
    fn bare_signal_name_accepted_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-KILL", "123"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGKILL.as_raw(),
                signal_origin: Some(Field::dummy("-KILL")),
                targets: Field::dummies(["123"]),
            })
        );
    }

    #[test]
    fn bare_signal_number_accepted_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-9", "123"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: 9,
                signal_origin: Some(Field::dummy("-9")),
                targets: Field::dummies(["123"]),
            })
        );
    }

    #[test]
    fn separate_signal_number_argument_rejected_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-s", "9", "123"]));
        assert_eq!(
            result,
            Err(Error::NonPortableSignalNumber {
                field: Field::dummy("9"),
                number: 9,
            })
        );
    }

    #[test]
    fn unseparated_signal_number_argument_rejected_as_number_under_portable() {
        // The number is diagnosed rather than the attachment, so that the
        // suggested replacement is portable itself.
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-s9", "123"]));
        assert_eq!(
            result,
            Err(Error::NonPortableSignalNumber {
                field: Field::dummy("-s9"),
                number: 9,
            })
        );
    }

    #[test]
    fn non_portable_signal_number_report() {
        let error = Error::NonPortableSignalNumber {
            field: Field::dummy("9"),
            number: 9,
        };
        assert_eq!(error.to_string(), "non-portable signal number 9");
        let report = error.to_report();
        assert_eq!(report.footnotes.len(), 2);
        assert_eq!(
            report.footnotes[0].label,
            "this error is reported because the `portable` shell option is enabled"
        );
        assert_eq!(report.footnotes[1].label, "use `-9` instead");
    }

    #[test]
    fn negative_signal_number_report_has_no_suggestion() {
        let error = Error::NonPortableSignalNumber {
            field: Field::dummy("-5"),
            number: -5,
        };
        let report = error.to_report();
        assert_eq!(report.footnotes.len(), 1);
        assert_eq!(
            report.footnotes[0].label,
            "this error is reported because the `portable` shell option is enabled"
        );
    }

    #[test]
    fn signal_number_zero_argument_accepted_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-s", "0", "123"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: 0,
                signal_origin: Some(Field::dummy("0")),
                targets: Field::dummies(["123"]),
            })
        );
    }

    #[test]
    fn multiple_list_operands_rejected_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-l", "9", "15"]));
        assert_eq!(
            result,
            Err(Error::MultipleListOperands(
                Field::dummy("9"),
                Field::dummy("15"),
            ))
        );
    }

    #[test]
    fn signal_name_list_operand_rejected_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-l", "TERM"]));
        assert_eq!(
            result,
            Err(Error::NonPortableListOperand(Field::dummy("TERM")))
        );
    }

    #[test]
    fn invalid_list_operand_not_blamed_on_portable() {
        // An operand that is neither a number nor a signal name is invalid
        // regardless of the option, so it is left to the later validation in
        // the print module rather than reported as a portability violation.
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-l", "TERM1"]));
        assert_eq!(
            result,
            Ok(Command::Print {
                signals: Field::dummies(["TERM1"]),
                verbose: false,
            })
        );
    }

    #[test]
    fn single_numeric_list_operand_accepted_under_portable() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-l", "9"]));
        assert_eq!(
            result,
            Ok(Command::Print {
                signals: Field::dummies(["9"]),
                verbose: false,
            })
        );
    }

    #[test]
    fn sig_prefix_accepted_without_portable() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-s", "SIGINT", "123"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGINT.as_raw(),
                signal_origin: Some(Field::dummy("SIGINT")),
                targets: Field::dummies(["123"]),
            })
        );

        let result = parse(&env, Field::dummies(["-SIGINT", "123"]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: VirtualSystem::SIGINT.as_raw(),
                signal_origin: Some(Field::dummy("-SIGINT")),
                targets: Field::dummies(["123"]),
            })
        );
    }

    #[test]
    fn sig_prefix_rejected_under_portable() {
        let env = portable_env();

        let result = parse(&env, Field::dummies(["-s", "SIGINT", "123"]));
        assert_eq!(
            result,
            Err(Error::NonPortableSignalPrefix {
                field: Field::dummy("SIGINT"),
                name_index: 0,
            })
        );

        let result = parse(&env, Field::dummies(["-SIGINT", "123"]));
        assert_eq!(
            result,
            Err(Error::NonPortableSignalPrefix {
                field: Field::dummy("-SIGINT"),
                name_index: 1,
            })
        );
    }

    #[test]
    fn unseparated_sig_prefix_rejected_as_prefix_under_portable() {
        // The `SIG` prefix is diagnosed rather than the attachment, so that the
        // suggested replacement resolves both at once.
        let env = portable_env();

        let result = parse(&env, Field::dummies(["-sSIGINT", "123"]));
        assert_eq!(
            result,
            Err(Error::NonPortableSignalPrefix {
                field: Field::dummy("-sSIGINT"),
                name_index: 2,
            })
        );

        // Here the whole cluster, rather than the remainder, is the signal name.
        let result = parse(&env, Field::dummies(["-sigstop", "123"]));
        assert_eq!(
            result,
            Err(Error::NonPortableSignalPrefix {
                field: Field::dummy("-sigstop"),
                name_index: 1,
            })
        );
    }

    #[test]
    fn non_portable_signal_prefix_report() {
        let error = Error::NonPortableSignalPrefix {
            field: Field::dummy("SIGINT"),
            name_index: 0,
        };
        assert_eq!(error.to_string(), "non-portable signal name \"SIGINT\"");
        let report = error.to_report();
        assert_eq!(report.footnotes.len(), 2);
        assert_eq!(
            report.footnotes[0].label,
            "this error is reported because the `portable` shell option is enabled"
        );
        assert_eq!(report.footnotes[1].label, "use `-s INT` instead");
    }

    #[test]
    fn non_portable_signal_prefix_report_suggestions() {
        // Whatever form the signal name was given in, the suggestion is the
        // portable `-s NAME` form.
        let error = Error::NonPortableSignalPrefix {
            field: Field::dummy("-SIGINT"),
            name_index: 1,
        };
        let report = error.to_report();
        assert_eq!(report.footnotes[1].label, "use `-s INT` instead");

        let error = Error::NonPortableSignalPrefix {
            field: Field::dummy("-ssigstop"),
            name_index: 2,
        };
        // The title names the signal, not the whole field containing it.
        assert_eq!(error.to_string(), "non-portable signal name \"sigstop\"");
        let report = error.to_report();
        assert_eq!(report.footnotes[1].label, "use `-s STOP` instead");
    }

    #[test]
    fn leftmost_non_portable_field_is_reported() {
        let env = portable_env();
        let result = parse(&env, Field::dummies(["-n", "TERM", "-sINT", "123"]));
        assert_eq!(
            result,
            Err(Error::NonPortableOption('n', Field::dummy("-n")))
        );

        let result = parse(&env, Field::dummies(["-sINT", "-n", "TERM", "123"]));
        assert_eq!(
            result,
            Err(Error::UnseparatedSignalArgument {
                field: Field::dummy("-sINT"),
                argument_index: 2,
            })
        );
    }
}
