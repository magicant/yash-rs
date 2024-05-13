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

//! Items that define trap conditions

pub mod signal;

#[cfg(doc)]
use super::state::Action;
#[doc(no_inline)]
pub use nix::sys::signal::Signal;
use std::ffi::c_int;

/// Condition under which an [`Action`] is executed
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Condition {
    /// When the shell exits
    Exit,
    /// When the specified signal is delivered to the shell process
    Signal(Signal),
}

/// Conversion from `Signal` to `Condition`
impl From<Signal> for Condition {
    fn from(signal: Signal) -> Self {
        Self::Signal(signal)
    }
}

/// Conversion from `Condition` to `String`
///
/// The result is an uppercase string representing the condition such as
/// `"EXIT"` and `"TERM"`.
impl std::fmt::Display for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Condition::Exit => "EXIT".fmt(f),
            Condition::Signal(signal) => {
                let full_name = signal.as_str();
                let name = full_name.strip_prefix("SIG").unwrap_or(full_name);
                name.fmt(f)
            }
        }
    }
}

/// Error in conversion from string to [`Condition`]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ParseConditionError;

/// Conversion from `String` to `Condition`
///
/// This implementation supports parsing uppercase strings like `"EXIT"` and
/// `"TERM"` as well as signal numbers like `"9"` and `"15"`. The number `"0"`
/// denotes [`Condition::Exit`].
impl std::str::FromStr for Condition {
    type Err = ParseConditionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // TODO Make case-insensitive
        // TODO Allow SIG-prefix
        // TODO Support real-time signals

        if let Ok(number) = s.parse::<c_int>() {
            if number == 0 {
                return Ok(Self::Exit);
            }
            return match number.try_into() {
                Ok(signal) => Ok(Self::Signal(signal)),
                Err(_) => Err(ParseConditionError),
            };
        }

        match s {
            "EXIT" => Ok(Self::Exit),
            _ => match format!("SIG{s}").parse() {
                Ok(signal) => Ok(Self::Signal(signal)),
                Err(_) => Err(ParseConditionError),
            },
        }
    }
}

#[test]
fn condition_from_str() {
    assert_eq!("EXIT".parse(), Ok(Condition::Exit));
    assert_eq!("TERM".parse(), Ok(Condition::Signal(Signal::SIGTERM)));
    assert_eq!("INT".parse(), Ok(Condition::Signal(Signal::SIGINT)));

    assert_eq!("0".parse(), Ok(Condition::Exit));
    assert_eq!("1".parse(), Ok(Condition::Signal(Signal::SIGHUP)));
    assert_eq!("9".parse(), Ok(Condition::Signal(Signal::SIGKILL)));

    assert_eq!("XXXXX".parse::<Condition>(), Err(ParseConditionError));
    assert_eq!("999999999".parse::<Condition>(), Err(ParseConditionError));
    assert_eq!("-123".parse::<Condition>(), Err(ParseConditionError));
}
