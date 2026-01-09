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

//! Definition of signal arguments
//!
//! This module defines the [`Signal`] type representing a signal to be sent or
//! printed by the kill built-in.

use std::borrow::Cow;
use std::fmt::Display;
use std::str::FromStr;
use yash_env::semantics::ExitStatus;
use yash_env::signal::{Name, Number, RawNumber};
use yash_env::system::Signals;

/// Specification of a signal to be sent by the kill built-in
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Signal {
    /// A signal specified by name
    Name(Name),
    /// A signal specified by raw number
    Number(RawNumber),
}

/// Parses a signal from a string
///
/// The string is parsed as a signal name or number. If the string is a number,
/// it is parsed as a raw number. Otherwise, it is parsed as a signal name
/// case-insensitively. The signal name must be specified without the `SIG`
/// prefix.
impl FromStr for Signal {
    type Err = <Name as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(number) = s.parse() {
            Ok(Self::Number(number))
        } else {
            let mut s = Cow::Borrowed(s);
            if s.contains(|c: char| c.is_ascii_lowercase()) {
                s.to_mut().make_ascii_uppercase();
            }
            Ok(Self::Name(s.parse()?))
        }
    }
}

impl Display for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(name) => name.fmt(f),
            Self::Number(number) => number.fmt(f),
        }
    }
}

/// Error indicating that a [`Signal`] is not supported
///
/// This error may be returned from [`Signal::to_number`] when the signal is not
/// supported by the system.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UnsupportedSignal;

impl Signal {
    /// Returns the signal number to be sent.
    ///
    /// If the signal is specified by name, the signal number is resolved from
    /// the name. If the signal is specified by number, the number is validated
    /// and returned. The special signal number 0 represents a dummy signal that
    /// is not actually sent. The function returns `Ok(None)` for this signal.
    ///
    /// If the signal is not supported by the system, the function returns an
    /// error.
    pub fn to_number<S: Signals>(self, system: &S) -> Result<Option<Number>, UnsupportedSignal> {
        match self {
            Signal::Name(name) => match system.signal_number_from_name(name) {
                Some(number) => Ok(Some(number)),
                None => Err(UnsupportedSignal),
            },
            Signal::Number(0) => Ok(None),
            Signal::Number(number) => match system.validate_signal(number) {
                Some((_name, number)) => Ok(Some(number)),
                None => Err(UnsupportedSignal),
            },
        }
    }

    /// Returns the signal name and number to be printed.
    ///
    /// If the signal is specified by name, the signal number is resolved from
    /// the name. If the signal is specified by number, the number is interpreted
    /// as an exit status and converted to a signal number. The function returns
    /// `None` if `self` does not represent a valid signal.
    #[must_use]
    pub fn to_name_and_number<S: Signals>(self, system: &S) -> Option<(Name, Number)> {
        match self {
            Signal::Name(name) => Some((name, system.signal_number_from_name(name)?)),
            Signal::Number(number) => {
                ExitStatus(number).to_signal(system, /* exact = */ false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZero;
    use yash_env::signal::UnknownNameError;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_env::system::r#virtual::{SIGHUP, SIGINT, SIGRTMAX, SIGRTMIN};

    #[test]
    fn signal_from_str_number() {
        assert_eq!("0".parse(), Ok(Signal::Number(0)));
        assert_eq!("1".parse(), Ok(Signal::Number(1)));
        assert_eq!("999".parse(), Ok(Signal::Number(999)));
    }

    #[test]
    fn signal_from_str_uppercase_name() {
        assert_eq!("HUP".parse(), Ok(Signal::Name(Name::Hup)));
        assert_eq!("INT".parse(), Ok(Signal::Name(Name::Int)));
        assert_eq!("QUIT".parse(), Ok(Signal::Name(Name::Quit)));
    }

    #[test]
    fn signal_from_str_lowercase_name() {
        assert_eq!("hup".parse(), Ok(Signal::Name(Name::Hup)));
        assert_eq!("int".parse(), Ok(Signal::Name(Name::Int)));
        assert_eq!("quit".parse(), Ok(Signal::Name(Name::Quit)));
    }

    #[test]
    fn signal_from_str_mixed_case_name() {
        assert_eq!("Hup".parse(), Ok(Signal::Name(Name::Hup)));
        assert_eq!("iNt".parse(), Ok(Signal::Name(Name::Int)));
        assert_eq!("quIT".parse(), Ok(Signal::Name(Name::Quit)));
    }

    #[test]
    fn signal_from_str_name_with_sig_prefix() {
        assert_eq!("SIGHUP".parse::<Signal>(), Err(UnknownNameError));
    }

    #[test]
    fn signal_name_to_number_supported() {
        let system = VirtualSystem::new();
        assert_eq!(Signal::Name(Name::Hup).to_number(&system), Ok(Some(SIGHUP)));
        assert_eq!(Signal::Name(Name::Int).to_number(&system), Ok(Some(SIGINT)));

        let next = Number::from_raw_unchecked(NonZero::new(SIGRTMIN.as_raw() + 1).unwrap());
        assert_eq!(
            Signal::Name(Name::Rtmin(1)).to_number(&system),
            Ok(Some(next))
        );
        let prev = Number::from_raw_unchecked(NonZero::new(SIGRTMAX.as_raw() - 1).unwrap());
        assert_eq!(
            Signal::Name(Name::Rtmax(-1)).to_number(&system),
            Ok(Some(prev))
        );
    }

    #[test]
    fn signal_name_to_number_unsupported() {
        let system = VirtualSystem::new();
        assert_eq!(
            Signal::Name(Name::Rtmin(-1)).to_number(&system),
            Err(UnsupportedSignal)
        );
        assert_eq!(
            Signal::Name(Name::Rtmax(1)).to_number(&system),
            Err(UnsupportedSignal)
        );
    }

    #[test]
    fn signal_0_to_number() {
        let system = VirtualSystem::new();
        assert_eq!(Signal::Number(0).to_number(&system), Ok(None));
    }

    #[test]
    fn signal_number_to_number_supported() {
        let system = VirtualSystem::new();
        assert_eq!(
            Signal::Number(SIGHUP.as_raw()).to_number(&system),
            Ok(Some(SIGHUP))
        );
        assert_eq!(
            Signal::Number(SIGINT.as_raw()).to_number(&system),
            Ok(Some(SIGINT))
        );

        let next = Number::from_raw_unchecked(NonZero::new(SIGRTMIN.as_raw() + 1).unwrap());
        assert_eq!(
            Signal::Number(next.as_raw()).to_number(&system),
            Ok(Some(next))
        );
        let prev = Number::from_raw_unchecked(NonZero::new(SIGRTMAX.as_raw() - 1).unwrap());
        assert_eq!(
            Signal::Number(prev.as_raw()).to_number(&system),
            Ok(Some(prev))
        );
    }

    #[test]
    fn signal_number_to_number_unsupported() {
        let system = VirtualSystem::new();
        assert_eq!(
            Signal::Number(-1).to_number(&system),
            Err(UnsupportedSignal)
        );
        assert_eq!(
            Signal::Number(RawNumber::MAX).to_number(&system),
            Err(UnsupportedSignal)
        );
    }
}
