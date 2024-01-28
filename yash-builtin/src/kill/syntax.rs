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
use thiserror::Error;
use yash_env::semantics::Field;
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

/// Parse command line arguments
pub fn parse(env: &Env, args: Vec<Field>) -> Result<Command, Error> {
    _ = env;
    _ = args;
    // TODO
    Err(Error::MissingTarget)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "TODO"]
    fn unknown_option() {}

    #[test]
    #[ignore = "TODO"]
    fn option_s_conflicts_with_option_l() {}

    #[test]
    #[ignore = "TODO"]
    fn option_n_conflicts_with_option_l() {}

    #[test]
    #[ignore = "TODO"]
    fn option_s_conflicts_with_option_v() {}

    #[test]
    #[ignore = "TODO"]
    fn option_n_conflicts_with_option_v() {}

    #[test]
    #[ignore = "TODO"]
    fn option_s_without_signal() {}

    #[test]
    #[ignore = "TODO"]
    fn option_n_without_signal() {}

    #[test]
    #[ignore = "TODO"]
    fn multiple_signals_error() {}

    #[test]
    #[ignore = "TODO"]
    fn invalid_signal_argument_to_option_s() {}

    #[test]
    #[ignore = "TODO"]
    fn invalid_signal_argument_to_option_n() {}

    #[test]
    #[ignore = "TODO"]
    fn invalid_signal_operand_with_option_l() {}

    #[test]
    #[ignore = "TODO"]
    fn invalid_signal_operand_with_option_v() {}

    #[test]
    fn missing_target() {
        let env = Env::new_virtual();
        let result = parse(&env, vec![]);
        assert_eq!(result, Err(Error::MissingTarget));
    }
}
