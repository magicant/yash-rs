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
use std::ffi::c_int;
use thiserror::Error;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::trap::Signal;
use yash_env::Env;
use yash_syntax::source::pretty::{Annotation, AnnotationType, Message};
use yash_syntax::source::Location;

/// Error that may occur during parsing
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// An argument starts with a hyphen (`-`) but is not a valid option.
    #[error("unknown option")]
    UnknownOption(Field),

    /// The signal to send is specified and the `-l` or `-v` options is also
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

    /// No target is specified and the `-l` or `-v` option is not specified.
    #[error("no target process specified")]
    MissingTarget,
}

impl Error {
    /// Converts this error to a printable message
    pub fn to_message(&self) -> Message {
        let title = self.to_string().into();
        let annotations = match self {
            Error::UnknownOption(field) => vec![Annotation::new(
                AnnotationType::Error,
                format!("{:?} is not a valid option", field.value).into(),
                &field.origin,
            )],

            Error::ConflictingOptions {
                signal_arg,
                list_option_name,
                list_option_location,
            } => vec![
                Annotation::new(
                    AnnotationType::Error,
                    "signal to send is specified here".into(),
                    &signal_arg.origin,
                ),
                Annotation::new(
                    AnnotationType::Error,
                    format!("option `{list_option_name}` is incompatible").into(),
                    list_option_location,
                ),
            ],

            Error::MissingSignal {
                signal_option_name,
                signal_option_location,
            } => {
                vec![Annotation::new(
                    AnnotationType::Error,
                    format!("option `{signal_option_name}` requires a signal name or number")
                        .into(),
                    signal_option_location,
                )]
            }

            Error::MultipleSignals(field_1, field_2) => vec![
                Annotation::new(
                    AnnotationType::Error,
                    format!("first signal {:?}", field_1.value).into(),
                    &field_1.origin,
                ),
                Annotation::new(
                    AnnotationType::Error,
                    format!("second signal {:?}", field_2.value).into(),
                    &field_2.origin,
                ),
            ],

            Error::InvalidSignal(field) => vec![Annotation::new(
                AnnotationType::Error,
                format!("{:?} is not a valid signal name or number", field.value).into(),
                &field.origin,
            )],

            Error::MissingTarget => vec![],
        };

        Message {
            r#type: AnnotationType::Error,
            title,
            annotations,
            footers: vec![],
        }
    }
}

/// Converts a string to a signal.
///
/// The string should be a signal name.
///
/// If the string is a valid signal name, this function returns `Some(signal)`.
/// Otherwise, this function returns `None`.
///
/// The signal name is parsed case-insensitively.
///
/// If `allow_sig_prefix` is `true`, the `SIG` prefix is optional for signal
/// names. Otherwise, the `SIG` prefix must **not** be present.
#[must_use]
pub fn parse_signal_name(s: &str, allow_sig_prefix: bool) -> Option<Signal> {
    let mut name = String::with_capacity(s.len() + 3);
    name.push_str(s);
    name.make_ascii_uppercase();
    if !allow_sig_prefix || !name.starts_with("SIG") {
        name.insert_str(0, "SIG");
    }
    name.parse().ok()
}

/// Converts a string to a signal.
///
/// The string may be a signal name or a signal number.
///
/// If the string is a valid signal name or number, this function returns
/// `Some(Some(signal))`. If the string represents the dummy signal number `0`,
/// this function returns `Some(None)`. Otherwise, this function returns `None`.
///
/// The signal name is parsed case-insensitively.
///
/// If `allow_sig_prefix` is `true`, the `SIG` prefix is optional for signal
/// names. Otherwise, the `SIG` prefix must **not** be present.
#[must_use]
pub fn parse_signal_name_or_number(s: &str, allow_sig_prefix: bool) -> Option<Option<Signal>> {
    if let Ok(number) = s.parse::<c_int>() {
        if number == 0 {
            Some(None)
        } else {
            number.try_into().ok().map(Some)
        }
    } else {
        parse_signal_name(s, allow_sig_prefix).map(Some)
    }
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
    signal: &mut Option<Signal>,
    signal_origin: &mut Option<Field>,
    new_signal: Option<Option<Signal>>,
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

/// Parses operands to the `-l` or `-v` option.
fn parse_signals<I: Iterator<Item = Field>>(
    operands: I,
    allow_sig_prefix: bool,
) -> Result<Vec<Signal>, Error> {
    let parse_one = move |operand: Field| {
        if let Some(exit_status) = operand.value.parse().ok().map(ExitStatus) {
            exit_status.try_into().ok()
        } else {
            parse_signal_name(&operand.value, allow_sig_prefix)
        }
        .ok_or(Error::InvalidSignal(operand))
    };

    operands.map(parse_one).collect()
}

/// Parse the case where the `-l` or `-v` option is specified.
fn parse_list_case<I: Iterator<Item = Field>>(
    operands: I,
    allow_sig_prefix: bool,
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
        let signals = parse_signals(operands, allow_sig_prefix)?;
        Ok(Command::Print { signals, verbose })
    }
}

/// Parses command line arguments.
pub fn parse(_env: &Env, args: Vec<Field>) -> Result<Command, Error> {
    let allow_sig_prefix = false; // TODO true depending on the shell option
    let mut args = args.into_iter().peekable();
    let mut signal = Some(Signal::SIGTERM);
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
                's' | 'n' => {
                    let remainder = chars.as_str();
                    if remainder.is_empty() {
                        let Some(current_signal_arg) = args.next() else {
                            return Err(Error::MissingSignal {
                                signal_option_name: option,
                                signal_option_location: arg.origin,
                            });
                        };
                        set_signal(
                            &mut signal,
                            &mut signal_origin,
                            parse_signal_name_or_number(
                                &current_signal_arg.value,
                                allow_sig_prefix,
                            ),
                            current_signal_arg,
                        )?;
                    } else {
                        set_signal(
                            &mut signal,
                            &mut signal_origin,
                            parse_signal_name_or_number(remainder, allow_sig_prefix)
                                .or_else(|| parse_signal_name_or_number(options, allow_sig_prefix)),
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
                    set_signal(
                        &mut signal,
                        &mut signal_origin,
                        parse_signal_name_or_number(options, allow_sig_prefix),
                        arg,
                    )
                    .map_err(invalid_signal_to_unknown_option)?;
                    break;
                }
            }
        }
    }

    // Parse operands and compute the result
    if let Some(option_location) = verbose {
        parse_list_case(
            args,
            allow_sig_prefix,
            signal_origin,
            'v',
            option_location,
            true,
        )
    } else if let Some(option_location) = list {
        parse_list_case(
            args,
            allow_sig_prefix,
            signal_origin,
            'l',
            option_location,
            false,
        )
    } else {
        // Command::Send case
        if args.peek().is_none() {
            Err(Error::MissingTarget)
        } else {
            let targets = args.collect();
            Ok(Command::Send { signal, targets })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_signal_names() {
        assert_eq!(parse_signal_name("TERM", false), Some(Signal::SIGTERM));
        assert_eq!(parse_signal_name("TeRm", false), Some(Signal::SIGTERM));
        assert_eq!(parse_signal_name("INT", false), Some(Signal::SIGINT));
        assert_eq!(parse_signal_name("uSr2", false), Some(Signal::SIGUSR2));

        // When `allow_sig_prefix` is `true`, the `SIG` prefix is optional.
        assert_eq!(parse_signal_name("tErM", true), Some(Signal::SIGTERM));
        assert_eq!(parse_signal_name("sIgtErM", true), Some(Signal::SIGTERM));

        // When `allow_sig_prefix` is `false`, the `SIG` prefix must not be present.
        assert_eq!(parse_signal_name("sIgtErM", false), None);
    }

    #[test]
    fn parse_signal_numbers() {
        assert_eq!(parse_signal_name_or_number("0", false), Some(None));
        assert_eq!(
            parse_signal_name_or_number("1", false),
            Some(Some(Signal::SIGHUP))
        );
        assert_eq!(
            parse_signal_name_or_number("2", false),
            Some(Some(Signal::SIGINT))
        );
        assert_eq!(
            parse_signal_name_or_number("3", false),
            Some(Some(Signal::SIGQUIT))
        );
        assert_eq!(
            parse_signal_name_or_number("6", false),
            Some(Some(Signal::SIGABRT))
        );
        assert_eq!(
            parse_signal_name_or_number("9", false),
            Some(Some(Signal::SIGKILL))
        );
        assert_eq!(
            parse_signal_name_or_number("14", false),
            Some(Some(Signal::SIGALRM))
        );
        assert_eq!(
            parse_signal_name_or_number("15", false),
            Some(Some(Signal::SIGTERM))
        );
    }

    #[test]
    fn parse_signal_name_or_number_errors() {
        assert_eq!(parse_signal_name_or_number("", false), None);
        assert_eq!(parse_signal_name_or_number("TERM1", false), None);
        assert_eq!(parse_signal_name_or_number("1TERM", false), None);
        assert_eq!(parse_signal_name_or_number("-1", false), None);
    }

    #[test]
    fn empty_operand() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies([""]));
        assert_eq!(
            result,
            Ok(Command::Send {
                signal: Some(Signal::SIGTERM),
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
                signal: Some(Signal::SIGTERM),
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
                signal: Some(Signal::SIGINT),
                targets: Field::dummies(["0"]),
            })
        );

        let result = parse(&env, Field::dummies(["-l", "--", "9"]));
        assert_eq!(
            result,
            Ok(Command::Print {
                signals: vec![Signal::SIGKILL],
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
                signal: Some(Signal::SIGQUIT),
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
                signal: Some(Signal::SIGQUIT),
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
                signal: Some(Signal::SIGKILL),
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
                signal: Some(Signal::SIGQUIT),
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
                signal: Some(Signal::SIGKILL),
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
                signal: Some(Signal::SIGSTOP),
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
                signal: Some(Signal::SIGKILL),
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
        let exit_status = &ExitStatus::from(Signal::SIGQUIT).to_string();
        let result = parse(&env, Field::dummies(["-l", "TERM", "1", exit_status]));
        assert_eq!(
            result,
            Ok(Command::Print {
                signals: vec![Signal::SIGTERM, Signal::SIGHUP, Signal::SIGQUIT],
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
        let result = parse(&env, Field::dummies(["-n", "-1", "123"]));
        assert_eq!(result, Err(Error::InvalidSignal(Field::dummy("-1"))));
    }

    #[test]
    fn invalid_adjoined_signal_argument_to_option_s() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-sTERM1", "123"]));
        assert_eq!(result, Err(Error::InvalidSignal(Field::dummy("-sTERM1"))));
    }

    #[test]
    fn invalid_signal_operand_with_option_l() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-l", "TERM1"]));
        assert_eq!(result, Err(Error::InvalidSignal(Field::dummy("TERM1"))));

        let result = parse(&env, Field::dummies(["-l", "TERM", "0", "1"]));
        assert_eq!(result, Err(Error::InvalidSignal(Field::dummy("0"))));
    }

    #[test]
    fn invalid_signal_operand_with_option_v() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-v", "TERM1"]));
        assert_eq!(result, Err(Error::InvalidSignal(Field::dummy("TERM1"))));

        let result = parse(&env, Field::dummies(["-v", "TERM", "0", "1"]));
        assert_eq!(result, Err(Error::InvalidSignal(Field::dummy("0"))));
    }

    #[test]
    fn missing_target() {
        let env = Env::new_virtual();
        let result = parse(&env, vec![]);
        assert_eq!(result, Err(Error::MissingTarget));
    }
}
