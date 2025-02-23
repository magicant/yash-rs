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

//! Implementation of `Command::Print`
//!
//! The [`print`] function prepares the output of the `Print` command.
//! The [`execute`] function calls [`print`] and actually prints the output.
//!
//! [`print`]: print()

use super::Signal;
use crate::common::{report_failure, to_single_message};
use std::borrow::Cow;
use std::fmt::Write;
use std::num::NonZero;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::signal::{Name, Number};
use yash_env::system::System;
use yash_env::system::SystemEx;
use yash_syntax::source::pretty::{Annotation, AnnotationType, MessageBase};

/// Returns an iterator over all supported signals.
///
/// The iterator yields non-real-time signals first, followed by real-time signals.
// TODO Most part of this function is duplicated in yash_env::trap::Condition::iter.
// Consider refactoring to avoid duplication. Note that the two functions require
// different trait bounds.
fn all_signals<S: System>(system: &S) -> impl Iterator<Item = (Name, Number)> + '_ {
    let non_real_time = Name::iter()
        .filter(|name| !matches!(name, Name::Rtmin(_) | Name::Rtmax(_)))
        .filter_map(|name| Some((name, system.signal_number_from_name(name)?)));

    let rtmin = system.signal_number_from_name(Name::Rtmin(0));
    let rtmax = system.signal_number_from_name(Name::Rtmax(0));
    let range = if let (Some(rtmin), Some(rtmax)) = (rtmin, rtmax) {
        rtmin.as_raw()..=rtmax.as_raw()
    } else {
        #[allow(clippy::reversed_empty_ranges)]
        {
            0..=-1
        }
    };
    let real_time = range.into_iter().map(|n| {
        let number = Number::from_raw_unchecked(NonZero::new(n).unwrap());
        let name = system.signal_name_from_number(number);
        (name, number)
    });

    non_real_time.chain(real_time)
}

/// Writes the specified signal into the output string.
fn write_one_signal(name: Name, number: Number, verbose: bool, output: &mut String) {
    if verbose {
        // TODO Include the description of the signal
        writeln!(output, "{number}\t{name}").unwrap();
    } else {
        writeln!(output, "{name}").unwrap();
    }
}

/// Error indicating that a signal is not recognized.
///
/// This error may be returned from [`print`](print()).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{:?} does not represent a valid signal", .origin.value)]
pub struct InvalidSignal<'a> {
    /// The signal that is not recognized
    pub signal: Signal,
    /// The operand that specified the signal
    pub origin: &'a Field,
}

impl MessageBase for InvalidSignal<'_> {
    fn message_title(&self) -> Cow<str> {
        "unrecognized operand".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        Annotation::new(
            AnnotationType::Error,
            self.to_string().into(),
            &self.origin.origin,
        )
    }
}

/// Lists the specified signals into a string.
///
/// If `signals` is empty, all signals are listed.
/// If `signals` contains invalid signals, the function returns an error.
pub fn print<'a, S: SystemEx>(
    system: &S,
    signals: &'a [(Signal, Field)],
    verbose: bool,
) -> Result<String, Vec<InvalidSignal<'a>>> {
    let mut output = String::new();
    let mut errors = Vec::new();

    if signals.is_empty() {
        // Print all signals
        for (name, number) in all_signals(system) {
            write_one_signal(name, number, verbose, &mut output);
        }
    } else {
        // Print the specified signals
        for &(signal, ref origin) in signals {
            let Some((name, number)) = signal.to_name_and_number(system) else {
                errors.push(InvalidSignal { signal, origin });
                continue;
            };
            write_one_signal(name, number, verbose, &mut output);
        }
    }

    if errors.is_empty() {
        Ok(output)
    } else {
        Err(errors)
    }
}

/// Executes the `Print` command.
pub async fn execute(env: &mut Env, signals: &[(Signal, Field)], verbose: bool) -> crate::Result {
    match print(&env.system, signals, verbose) {
        Ok(output) => crate::common::output(env, &output).await,
        Err(errors) => report_failure(env, to_single_message(&errors).unwrap()).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::r#virtual::SIGKILL;
    use yash_env::system::r#virtual::VirtualSystem;

    #[test]
    fn print_one_non_verbose() {
        let system = &VirtualSystem::new();
        let signals = &[(Signal::Name(Name::Int), Field::dummy("INT"))];

        let result = print(system, signals, false).unwrap();
        assert_eq!(result, "INT\n");
    }

    #[test]
    fn print_some_non_verbose() {
        let system = &VirtualSystem::new();
        let signals = &[
            (Signal::Name(Name::Term), Field::dummy("SIGTERM")),
            (
                Signal::Number(ExitStatus::from(SIGKILL).0),
                Field::dummy("9"),
            ),
        ];

        let result = print(system, signals, false).unwrap();
        assert_eq!(result, "TERM\nKILL\n");
    }

    #[test]
    fn print_one_verbose() {
        let system = &VirtualSystem::new();
        let signals = &[(Signal::Name(Name::Int), Field::dummy("INT"))];

        let result = print(system, signals, true).unwrap();
        assert_eq!(result, "2\tINT\n");
    }

    #[test]
    fn print_some_unknown() {
        let system = &VirtualSystem::new();
        let signals = &[
            (Signal::Number(0), Field::dummy("0")),
            (Signal::Name(Name::Int), Field::dummy("INT")),
            (Signal::Name(Name::Rtmin(-1)), Field::dummy("RTMIN-1")),
        ];

        let errors = print(system, signals, false).unwrap_err();
        assert_eq!(
            errors,
            [
                InvalidSignal {
                    signal: Signal::Number(0),
                    origin: &Field::dummy("0")
                },
                InvalidSignal {
                    signal: Signal::Name(Name::Rtmin(-1)),
                    origin: &Field::dummy("RTMIN-1")
                },
            ]
        );
    }

    #[test]
    fn print_all_non_verbose() {
        let system = &VirtualSystem::new();
        let result = print(system, &[], false).unwrap();
        assert_eq!(
            result,
            "ABRT\nALRM\nBUS\nCHLD\nCLD\nCONT\nEMT\nFPE\nHUP\nILL\nINFO\nINT\n\
            IO\nIOT\nKILL\nLOST\nPIPE\nPOLL\nPROF\nPWR\nQUIT\nSEGV\nSTKFLT\n\
            STOP\nSYS\nTERM\nTHR\nTRAP\nTSTP\nTTIN\nTTOU\nURG\nUSR1\nUSR2\n\
            VTALRM\nWINCH\nXCPU\nXFSZ\nRTMIN\nRTMIN+1\nRTMIN+2\nRTMIN+3\n\
            RTMIN+4\nRTMAX-3\nRTMAX-2\nRTMAX-1\nRTMAX\n"
        );
    }
}
