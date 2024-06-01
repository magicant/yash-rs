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

#[cfg(doc)]
use super::state::Action;
use super::SignalSystem;
use crate::signal;
#[doc(no_inline)]
pub use nix::sys::signal::Signal;
use std::borrow::Cow;
use std::ffi::c_int;

/// Condition under which an [`Action`] is executed
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OldCondition {
    /// When the shell exits
    Exit,
    /// When the specified signal is delivered to the shell process
    Signal(Signal),
}

/// Conversion from `Signal` to `Condition`
impl From<Signal> for OldCondition {
    fn from(signal: Signal) -> Self {
        Self::Signal(signal)
    }
}

/// Conversion from `Condition` to `String`
///
/// The result is an uppercase string representing the condition such as
/// `"EXIT"` and `"TERM"`.
impl std::fmt::Display for OldCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OldCondition::Exit => "EXIT".fmt(f),
            OldCondition::Signal(signal) => {
                let full_name = signal.as_str();
                let name = full_name.strip_prefix("SIG").unwrap_or(full_name);
                name.fmt(f)
            }
        }
    }
}

// TODO Remove this
/// Error in conversion from string to [`Condition`]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ParseConditionError;

/// Conversion from `String` to `Condition`
///
/// This implementation supports parsing uppercase strings like `"EXIT"` and
/// `"TERM"` as well as signal numbers like `"9"` and `"15"`. The number `"0"`
/// denotes [`Condition::Exit`].
impl std::str::FromStr for OldCondition {
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
    assert_eq!("EXIT".parse(), Ok(OldCondition::Exit));
    assert_eq!("TERM".parse(), Ok(OldCondition::Signal(Signal::SIGTERM)));
    assert_eq!("INT".parse(), Ok(OldCondition::Signal(Signal::SIGINT)));

    assert_eq!("0".parse(), Ok(OldCondition::Exit));
    assert_eq!("1".parse(), Ok(OldCondition::Signal(Signal::SIGHUP)));
    assert_eq!("9".parse(), Ok(OldCondition::Signal(Signal::SIGKILL)));

    assert_eq!("XXXXX".parse::<OldCondition>(), Err(ParseConditionError));
    assert_eq!(
        "999999999".parse::<OldCondition>(),
        Err(ParseConditionError)
    );
    assert_eq!("-123".parse::<OldCondition>(), Err(ParseConditionError));
}

/// Condition under which an [`Action`] is executed
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Condition {
    /// When the shell exits
    Exit,
    /// When the specified signal is delivered to the shell process
    Signal(signal::Number),
}

impl From<signal::Number> for Condition {
    fn from(number: signal::Number) -> Self {
        Self::Signal(number)
    }
}

/// Conversion from raw signal number to `Condition`
///
/// If the number is zero, the result is [`Condition::Exit`]. Otherwise, the
/// result is [`Condition::Signal`] with the signal number.
impl From<signal::RawNumber> for Condition {
    fn from(number: signal::RawNumber) -> Self {
        if let Ok(non_zero) = number.try_into() {
            Self::Signal(signal::Number::from_raw_unchecked(non_zero))
        } else {
            Self::Exit
        }
    }
}

impl From<Condition> for signal::RawNumber {
    fn from(cond: Condition) -> Self {
        match cond {
            Condition::Exit => 0,
            Condition::Signal(number) => number.as_raw(),
        }
    }
}

impl Condition {
    /// Converts this `Condition` to a `String`.
    ///
    /// The result is an uppercase string representing the condition such as
    /// `"EXIT"` and `"TERM"`. Signal names are obtained from
    /// [`signal::Name::as_string`]. This function depends on the signal system
    /// to convert signal numbers to names.
    #[must_use]
    pub fn to_string<S: SignalSystem>(&self, system: &S) -> Cow<'static, str> {
        match self {
            Self::Exit => Cow::Borrowed("EXIT"),
            Self::Signal(number) => system.signal_name_from_number(*number).as_string(),
        }
    }
}
