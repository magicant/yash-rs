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

use std::ffi::c_int;
use std::fmt::Write;
use yash_env::trap::Signal;
use yash_env::Env;

/// Lists the specified signals into a string.
///
/// If `signals` is empty, all signals are listed.
#[must_use]
pub fn print(signals: &[Signal], verbose: bool) -> String {
    let mut specified = signals.iter().copied();
    let mut all = Signal::iterator();
    let iter: &mut dyn Iterator<Item = Signal> = if signals.is_empty() {
        &mut all
    } else {
        &mut specified
    };

    let mut result = String::new();
    for signal in iter {
        let name = signal.as_str();
        let name = name.strip_prefix("SIG").unwrap_or(name);
        if verbose {
            let number = signal as c_int;
            // TODO Include the description of the signal
            writeln!(result, "{number}\t{name}").unwrap();
        } else {
            writeln!(result, "{name}").unwrap();
        }
    }
    result
}

/// Executes the `Print` command.
pub async fn execute(env: &mut Env, signals: &[Signal], verbose: bool) -> crate::Result {
    let result = print(signals, verbose);
    crate::common::output(env, &result).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_non_empty_non_verbose() {
        let result = print(&[Signal::SIGTERM, Signal::SIGINT], false);
        assert_eq!(result, "TERM\nINT\n");

        let result = print(&[Signal::SIGKILL, Signal::SIGTSTP, Signal::SIGQUIT], false);
        assert_eq!(result, "KILL\nTSTP\nQUIT\n");
    }

    #[test]
    fn print_non_empty_verbose() {
        let result = print(&[Signal::SIGTERM, Signal::SIGINT], true);
        assert_eq!(result, "15\tTERM\n2\tINT\n");

        let result = print(&[Signal::SIGKILL, Signal::SIGALRM, Signal::SIGQUIT], true);
        assert_eq!(result, "9\tKILL\n14\tALRM\n3\tQUIT\n");
    }

    #[test]
    fn print_all_non_verbose() {
        let result = print(&[], false);
        assert!(result.contains("HUP\n"), "result: {result:?}");
        assert!(result.contains("INT\n"), "result: {result:?}");
        assert!(result.contains("KILL\n"), "result: {result:?}");
        assert!(result.contains("QUIT\n"), "result: {result:?}");
        assert!(result.contains("STOP\n"), "result: {result:?}");
        assert!(result.contains("TERM\n"), "result: {result:?}");
        assert!(result.contains("TSTP\n"), "result: {result:?}");
        assert!(result.contains("TTIN\n"), "result: {result:?}");
        assert!(result.contains("TTOU\n"), "result: {result:?}");
    }
}
