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
use std::borrow::Cow;

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
