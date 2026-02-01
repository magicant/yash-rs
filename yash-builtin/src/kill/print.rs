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

use crate::common::report::{merge_reports, report_failure};
use std::borrow::Cow;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::{ExitStatus, Field};
use yash_env::signal::{Number, RawNumber};
use yash_env::source::pretty::{Report, ReportType, Snippet};
use yash_env::system::{Fcntl, Isatty, Signals, Write};

/// Returns an iterator over all supported signals.
///
/// The iterator yields pairs of signal names and numbers in the ascending order of
/// signal numbers. The iterator includes both real-time and non-real-time signals.
// TODO Most part of this function is duplicated in yash_env::trap::Condition::iter.
// Consider refactoring to avoid duplication. Note that Condition::iter
// deduplicates signals while this function does not.
fn all_signals<S: Signals>(
    system: &S,
) -> impl Iterator<Item = (Cow<'static, str>, Number)> + 'static {
    let non_real_time = S::NAMED_SIGNALS
        .iter()
        .filter_map(|&(name, number)| Some((Cow::Borrowed(name), number?)));
    let non_real_time_count = S::NAMED_SIGNALS.len();

    let real_time = system
        .iter_sigrt()
        .map(|number| (system.sig2str(number).unwrap(), number));
    let real_time_count = real_time.size_hint().1.unwrap_or_default();

    let mut signals = Vec::with_capacity(non_real_time_count + real_time_count);
    signals.extend(non_real_time);
    signals.extend(real_time);
    signals.sort_by_key(|&(_, number)| number);
    signals.into_iter()
}

/// Writes the specified signal into the output string.
fn write_one_signal(name: &str, number: Number, verbose: bool, output: &mut String) {
    use std::fmt::Write as _;
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
#[error("{:?} does not represent a valid signal", .0.value)]
pub struct InvalidSignal<'a>(
    /// The operand that specified the signal
    pub &'a Field,
);

impl InvalidSignal<'_> {
    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = "unrecognized operand".into();
        report.snippets = Snippet::with_primary_span(&self.0.origin, self.to_string().into());
        report
    }
}

impl<'a> From<&'a InvalidSignal<'a>> for Report<'a> {
    #[inline]
    fn from(error: &'a InvalidSignal<'a>) -> Self {
        error.to_report()
    }
}

/// Converts a signal specification string to a signal name and number.
fn to_name_and_number<'a, S: Signals>(system: &S, spec: &'a str) -> Option<(Cow<'a, str>, Number)> {
    // TODO Skip any SIG prefix when specified by name
    // TODO Case-insensitive comparison when specified by name
    if let Ok(number) = spec.parse::<RawNumber>() {
        // Specified by number
        ExitStatus(number).to_signal(system, /* exact = */ false)
    } else {
        // Specified by name
        system
            .str2sig(spec)
            .map(|number| (Cow::Borrowed(spec), number))
    }
}

/// Lists the specified signals into a string.
///
/// If `signals` is empty, all signals are listed.
/// If `signals` contains invalid signals, the function returns an error.
pub fn print<'a, S: Signals>(
    system: &S,
    signals: &'a [Field],
    verbose: bool,
) -> Result<String, Vec<InvalidSignal<'a>>> {
    let mut output = String::new();
    let mut errors = Vec::new();

    if signals.is_empty() {
        // Print all signals
        for (name, number) in all_signals(system) {
            write_one_signal(&name, number, verbose, &mut output);
        }
    } else {
        // Print the specified signals
        for signal_spec in signals {
            let Some((name, number)) = to_name_and_number(system, &signal_spec.value) else {
                errors.push(InvalidSignal(signal_spec));
                continue;
            };
            write_one_signal(&name, number, verbose, &mut output);
        }
    }

    if errors.is_empty() {
        Ok(output)
    } else {
        Err(errors)
    }
}

/// Executes the `Print` command.
pub async fn execute<S>(env: &mut Env<S>, signals: &[Field], verbose: bool) -> crate::Result
where
    S: Fcntl + Isatty + Signals + Write,
{
    match print(&env.system, signals, verbose) {
        Ok(output) => crate::common::output(env, &output).await,
        Err(errors) => report_failure(env, merge_reports(&errors).unwrap()).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::system::r#virtual::VirtualSystem;

    #[test]
    fn to_name_and_number_from_number() {
        let system = VirtualSystem::new();

        // Direct signal number
        let result = to_name_and_number(&system, "9");
        assert_eq!(
            result,
            Some((Cow::Borrowed("KILL"), VirtualSystem::SIGKILL))
        );

        // Exit status representing a signal
        let result = to_name_and_number(&system, "386");
        assert_eq!(result, Some((Cow::Borrowed("INT"), VirtualSystem::SIGINT)));

        // Invalid number
        let result = to_name_and_number(&system, "0");
        assert_eq!(result, None);
    }

    #[test]
    fn to_name_and_number_from_name() {
        let system = VirtualSystem::new();

        // Valid name
        let result = to_name_and_number(&system, "TERM");
        assert_eq!(
            result,
            Some((Cow::Borrowed("TERM"), VirtualSystem::SIGTERM))
        );

        // Invalid name
        let result = to_name_and_number(&system, "FOO");
        assert_eq!(result, None);
    }

    #[test]
    fn print_one_non_verbose() {
        let system = &VirtualSystem::new();
        let signals = Field::dummies(["INT"]);

        let result = print(system, &signals, false).unwrap();
        assert_eq!(result, "INT\n");
    }

    #[test]
    fn print_some_non_verbose() {
        let system = &VirtualSystem::new();
        let signals = Field::dummies(["TERM", "9"]);

        let result = print(system, &signals, false).unwrap();
        assert_eq!(result, "TERM\nKILL\n");
    }

    #[test]
    fn print_one_verbose() {
        let system = &VirtualSystem::new();
        let signals = Field::dummies(["INT"]);

        let result = print(system, &signals, true).unwrap();
        assert_eq!(result, "2\tINT\n");
    }

    #[test]
    fn print_some_unknown() {
        let system = &VirtualSystem::new();
        let signals = Field::dummies(["0", "INT", "RTMIN-1"]);

        let errors = print(system, &signals, false).unwrap_err();
        assert_eq!(
            errors,
            [
                InvalidSignal(&Field::dummy("0")),
                InvalidSignal(&Field::dummy("RTMIN-1")),
            ]
        );
    }

    #[test]
    fn print_all_non_verbose() {
        let system = &VirtualSystem::new();
        let result = print(system, &[], false).unwrap();
        assert_eq!(
            result,
            "HUP\nINT\nQUIT\nABRT\nIOT\nKILL\nALRM\nTERM\nBUS\nCHLD\nCLD\n\
            CONT\nEMT\nFPE\nILL\nINFO\nIO\nLOST\nPIPE\nPOLL\nPROF\nPWR\nSEGV\n\
            STKFLT\nSTOP\nSYS\nTHR\nTRAP\nTSTP\nTTIN\nTTOU\nURG\nUSR1\nUSR2\n\
            VTALRM\nWINCH\nXCPU\nXFSZ\nRTMIN\nRTMIN+1\nRTMIN+2\nRTMIN+3\n\
            RTMIN+4\nRTMAX-3\nRTMAX-2\nRTMAX-1\nRTMAX\n"
        );
    }
}
