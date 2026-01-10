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

//! Definition of `CondSpec`

use yash_env::signal;
use yash_env::system::Signals;
use yash_env::trap::Condition;

/// Interpretation of a command line operand that specifies a trap condition
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CondSpec {
    /// The `EXIT` condition
    Exit,
    /// A symbolic name of a signal
    SignalName(signal::Name),
    /// A signal number (or 0 for `EXIT`)
    Number(signal::RawNumber),
}

impl From<signal::Name> for CondSpec {
    fn from(name: signal::Name) -> Self {
        Self::SignalName(name)
    }
}

impl From<signal::RawNumber> for CondSpec {
    fn from(number: signal::RawNumber) -> Self {
        Self::Number(number)
    }
}

impl CondSpec {
    /// Converts this `CondSpec` to a `Condition`.
    ///
    /// If this `CondSpec` contains a signal name or number that is not
    /// supported by the system, this function returns `None`.
    #[must_use]
    pub fn to_condition<S: Signals>(&self, system: &S) -> Option<Condition> {
        match self {
            Self::Exit => Some(Condition::Exit),
            Self::SignalName(name) => {
                Some(Condition::Signal(system.signal_number_from_name(*name)?))
            }
            Self::Number(0) => Some(Condition::Exit),
            Self::Number(number) => Some(Condition::Signal(system.validate_signal(*number)?.1)),
        }
    }

    /// Converts a `Condition` to a `CondSpec`.
    ///
    /// If the `Condition` is of an unknown variant, this function returns `None`.
    #[must_use]
    pub fn from_condition<S: Signals>(cond: &Condition, system: &S) -> Option<Self> {
        match cond {
            Condition::Exit => Some(Self::Exit),
            Condition::Signal(number) => {
                Some(Self::SignalName(system.signal_name_from_number(*number)))
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for CondSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exit => "EXIT".fmt(f),
            Self::SignalName(name) => name.fmt(f),
            Self::Number(number) => number.fmt(f),
        }
    }
}

impl std::str::FromStr for CondSpec {
    type Err = signal::UnknownNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(number) = s.parse() {
            return Ok(Self::Number(number));
        }

        if s == "EXIT" {
            Ok(Self::Exit)
        } else {
            Ok(Self::SignalName(s.parse()?))
        }
    }
}
