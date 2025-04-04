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

//! Functions about signals

pub(super) use crate::signal::*;
use std::num::NonZero;

/// Signal number for `SIGABRT` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGABRT` is 6.
pub const SIGABRT: Number = Number::from_raw_unchecked(NonZero::new(6).unwrap());

/// Signal number for `SIGALRM` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGALRM` is 14.
pub const SIGALRM: Number = Number::from_raw_unchecked(NonZero::new(14).unwrap());

/// Signal number for `SIGBUS` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGBUS: Number = Number::from_raw_unchecked(NonZero::new(101).unwrap());

/// Signal number for `SIGCHLD` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGCHLD: Number = Number::from_raw_unchecked(NonZero::new(102).unwrap());

/// Signal number for `SIGCLD` in the virtual system
///
/// Currently, this is the same as `SIGCHLD`.
pub const SIGCLD: Number = SIGCHLD;

/// Signal number for `SIGCONT` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGCONT: Number = Number::from_raw_unchecked(NonZero::new(103).unwrap());

/// Signal number for `SIGEMT` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGEMT: Number = Number::from_raw_unchecked(NonZero::new(104).unwrap());

/// Signal number for `SIGFPE` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGFPE: Number = Number::from_raw_unchecked(NonZero::new(105).unwrap());

/// Signal number for `SIGHUP` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGHUP` is 1.
pub const SIGHUP: Number = Number::from_raw_unchecked(NonZero::new(1).unwrap());

/// Signal number for `SIGILL` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGILL: Number = Number::from_raw_unchecked(NonZero::new(106).unwrap());

/// Signal number for `SIGINFO` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGINFO: Number = Number::from_raw_unchecked(NonZero::new(107).unwrap());

/// Signal number for `SIGINT` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGINT` is 2.
pub const SIGINT: Number = Number::from_raw_unchecked(NonZero::new(2).unwrap());

/// Signal number for `SIGIO` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGIO: Number = Number::from_raw_unchecked(NonZero::new(108).unwrap());

/// Signal number for `SIGIOT` in the virtual system
///
/// Currently, this is the same as `SIGABRT`.
pub const SIGIOT: Number = SIGABRT;

/// Signal number for `SIGKILL` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGKILL` is 9.
pub const SIGKILL: Number = Number::from_raw_unchecked(NonZero::new(9).unwrap());

/// Signal number for `SIGLOST` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGLOST: Number = Number::from_raw_unchecked(NonZero::new(109).unwrap());

/// Signal number for `SIGPIPE` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGPIPE: Number = Number::from_raw_unchecked(NonZero::new(110).unwrap());

/// Signal number for `SIGPOLL` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGPOLL: Number = Number::from_raw_unchecked(NonZero::new(111).unwrap());

/// Signal number for `SIGPROF` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGPROF: Number = Number::from_raw_unchecked(NonZero::new(112).unwrap());

/// Signal number for `SIGPWR` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGPWR: Number = Number::from_raw_unchecked(NonZero::new(113).unwrap());

/// Signal number for `SIGQUIT` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGQUIT` is 3.
pub const SIGQUIT: Number = Number::from_raw_unchecked(NonZero::new(3).unwrap());

/// Signal number for `SIGSEGV` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGSEGV: Number = Number::from_raw_unchecked(NonZero::new(114).unwrap());

/// Signal number for `SIGSTKFLT` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGSTKFLT: Number = Number::from_raw_unchecked(NonZero::new(115).unwrap());

/// Signal number for `SIGSTOP` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGSTOP: Number = Number::from_raw_unchecked(NonZero::new(116).unwrap());

/// Signal number for `SIGSYS` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGSYS: Number = Number::from_raw_unchecked(NonZero::new(117).unwrap());

/// Signal number for `SIGTERM` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGTERM` is 15.
pub const SIGTERM: Number = Number::from_raw_unchecked(NonZero::new(15).unwrap());

/// Signal number for `SIGTHR` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGTHR: Number = Number::from_raw_unchecked(NonZero::new(118).unwrap());

/// Signal number for `SIGTRAP` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGTRAP: Number = Number::from_raw_unchecked(NonZero::new(119).unwrap());

/// Signal number for `SIGTSTP` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGTSTP: Number = Number::from_raw_unchecked(NonZero::new(120).unwrap());

/// Signal number for `SIGTTIN` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGTTIN: Number = Number::from_raw_unchecked(NonZero::new(121).unwrap());

/// Signal number for `SIGTTOU` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGTTOU: Number = Number::from_raw_unchecked(NonZero::new(122).unwrap());

/// Signal number for `SIGURG` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGURG: Number = Number::from_raw_unchecked(NonZero::new(123).unwrap());

/// Signal number for `SIGUSR1` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGUSR1: Number = Number::from_raw_unchecked(NonZero::new(124).unwrap());

/// Signal number for `SIGUSR2` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGUSR2: Number = Number::from_raw_unchecked(NonZero::new(125).unwrap());

/// Signal number for `SIGVTALRM` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGVTALRM: Number = Number::from_raw_unchecked(NonZero::new(126).unwrap());

/// Signal number for `SIGWINCH` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGWINCH: Number = Number::from_raw_unchecked(NonZero::new(127).unwrap());

/// Signal number for `SIGXCPU` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGXCPU: Number = Number::from_raw_unchecked(NonZero::new(128).unwrap());

/// Signal number for `SIGXFSZ` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGXFSZ: Number = Number::from_raw_unchecked(NonZero::new(129).unwrap());

/// Signal number for `SIGRTMIN` in the virtual system
///
/// The current implementation supports nine real-time signals (201 through 209).
pub const SIGRTMIN: Number = Number::from_raw_unchecked(NonZero::new(201).unwrap());

/// Signal number for `SIGRTMAX` in the virtual system
///
/// The current implementation supports nine real-time signals (201 through 209).
pub const SIGRTMAX: Number = Number::from_raw_unchecked(NonZero::new(209).unwrap());

/// Range of the real-time signals supported by the virtual system.
const RT_RANGE: std::ops::RangeInclusive<RawNumber> = SIGRTMIN.as_raw()..=SIGRTMAX.as_raw();

impl Name {
    pub(super) fn to_raw_virtual(self) -> Option<Number> {
        fn rt(base: Number, n: RawNumber) -> Option<Number> {
            let number = base.as_raw().checked_add(n)?;
            let non_zero = NonZero::new(number)?;
            RT_RANGE
                .contains(&number)
                .then(|| Number::from_raw_unchecked(non_zero))
        }

        match self {
            Self::Abrt => Some(SIGABRT),
            Self::Alrm => Some(SIGALRM),
            Self::Bus => Some(SIGBUS),
            Self::Chld => Some(SIGCHLD),
            Self::Cld => Some(SIGCLD),
            Self::Cont => Some(SIGCONT),
            Self::Emt => Some(SIGEMT),
            Self::Fpe => Some(SIGFPE),
            Self::Hup => Some(SIGHUP),
            Self::Ill => Some(SIGILL),
            Self::Info => Some(SIGINFO),
            Self::Int => Some(SIGINT),
            Self::Io => Some(SIGIO),
            Self::Iot => Some(SIGIOT),
            Self::Kill => Some(SIGKILL),
            Self::Lost => Some(SIGLOST),
            Self::Pipe => Some(SIGPIPE),
            Self::Poll => Some(SIGPOLL),
            Self::Prof => Some(SIGPROF),
            Self::Pwr => Some(SIGPWR),
            Self::Quit => Some(SIGQUIT),
            Self::Segv => Some(SIGSEGV),
            Self::Stkflt => Some(SIGSTKFLT),
            Self::Stop => Some(SIGSTOP),
            Self::Sys => Some(SIGSYS),
            Self::Term => Some(SIGTERM),
            Self::Thr => Some(SIGTHR),
            Self::Trap => Some(SIGTRAP),
            Self::Tstp => Some(SIGTSTP),
            Self::Ttin => Some(SIGTTIN),
            Self::Ttou => Some(SIGTTOU),
            Self::Urg => Some(SIGURG),
            Self::Usr1 => Some(SIGUSR1),
            Self::Usr2 => Some(SIGUSR2),
            Self::Vtalrm => Some(SIGVTALRM),
            Self::Winch => Some(SIGWINCH),
            Self::Xcpu => Some(SIGXCPU),
            Self::Xfsz => Some(SIGXFSZ),
            Self::Rtmin(n) => rt(SIGRTMIN, n),
            Self::Rtmax(n) => rt(SIGRTMAX, n),
        }
    }

    /// Returns the name for the raw signal number for the virtual system.
    ///
    /// This function returns `None` if the given number is not a valid signal.
    pub(super) fn try_from_raw_virtual(number: RawNumber) -> Option<Self> {
        match () {
            () if number == SIGABRT.as_raw() => Some(Self::Abrt),
            () if number == SIGALRM.as_raw() => Some(Self::Alrm),
            () if number == SIGBUS.as_raw() => Some(Self::Bus),
            () if number == SIGCHLD.as_raw() => Some(Self::Chld),
            () if number == SIGCLD.as_raw() => Some(Self::Cld),
            () if number == SIGCONT.as_raw() => Some(Self::Cont),
            () if number == SIGEMT.as_raw() => Some(Self::Emt),
            () if number == SIGFPE.as_raw() => Some(Self::Fpe),
            () if number == SIGHUP.as_raw() => Some(Self::Hup),
            () if number == SIGILL.as_raw() => Some(Self::Ill),
            () if number == SIGINFO.as_raw() => Some(Self::Info),
            () if number == SIGINT.as_raw() => Some(Self::Int),
            () if number == SIGIO.as_raw() => Some(Self::Io),
            () if number == SIGIOT.as_raw() => Some(Self::Iot),
            () if number == SIGKILL.as_raw() => Some(Self::Kill),
            () if number == SIGLOST.as_raw() => Some(Self::Lost),
            () if number == SIGPIPE.as_raw() => Some(Self::Pipe),
            () if number == SIGPOLL.as_raw() => Some(Self::Poll),
            () if number == SIGPROF.as_raw() => Some(Self::Prof),
            () if number == SIGPWR.as_raw() => Some(Self::Pwr),
            () if number == SIGQUIT.as_raw() => Some(Self::Quit),
            () if number == SIGSEGV.as_raw() => Some(Self::Segv),
            () if number == SIGSTKFLT.as_raw() => Some(Self::Stkflt),
            () if number == SIGSTOP.as_raw() => Some(Self::Stop),
            () if number == SIGSYS.as_raw() => Some(Self::Sys),
            () if number == SIGTERM.as_raw() => Some(Self::Term),
            () if number == SIGTHR.as_raw() => Some(Self::Thr),
            () if number == SIGTRAP.as_raw() => Some(Self::Trap),
            () if number == SIGTSTP.as_raw() => Some(Self::Tstp),
            () if number == SIGTTIN.as_raw() => Some(Self::Ttin),
            () if number == SIGTTOU.as_raw() => Some(Self::Ttou),
            () if number == SIGURG.as_raw() => Some(Self::Urg),
            () if number == SIGUSR1.as_raw() => Some(Self::Usr1),
            () if number == SIGUSR2.as_raw() => Some(Self::Usr2),
            () if number == SIGVTALRM.as_raw() => Some(Self::Vtalrm),
            () if number == SIGWINCH.as_raw() => Some(Self::Winch),
            () if number == SIGXCPU.as_raw() => Some(Self::Xcpu),
            () if number == SIGXFSZ.as_raw() => Some(Self::Xfsz),
            () if RT_RANGE.contains(&number) => {
                // Return a name relative to `Rtmin` or `Rtmax`,
                // whichever is closer to the given number.
                let incr = number - SIGRTMIN.as_raw();
                debug_assert!(incr >= 0);
                let decr = number - SIGRTMAX.as_raw();
                debug_assert!(decr <= 0);
                debug_assert!(decr > RawNumber::MIN);
                if incr <= -decr {
                    Some(Self::Rtmin(incr))
                } else {
                    Some(Self::Rtmax(decr))
                }
            }
            _ => None,
        }
    }
}

/// Default effect of a signal delivered to a process.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SignalEffect {
    /// Does nothing.
    None,
    /// Terminates the process.
    Terminate { core_dump: bool },
    /// Suspends the process.
    Suspend,
    /// Resumes the process.
    Resume,
}

impl SignalEffect {
    /// Returns the default effect for the specified signal.
    ///
    /// This function returns `Terminate { core_dump: true }` for `Rtmin(n)` and
    /// `Rtmax(n)` whatever `n` is.
    #[must_use]
    pub const fn of(signal: Name) -> Self {
        match signal {
            Name::Abrt => Self::Terminate { core_dump: true },
            Name::Alrm => Self::Terminate { core_dump: false },
            Name::Bus => Self::Terminate { core_dump: true },
            Name::Chld | Name::Cld => Self::None,
            Name::Cont => Self::Resume,
            Name::Emt => Self::Terminate { core_dump: false },
            Name::Fpe => Self::Terminate { core_dump: true },
            Name::Hup => Self::Terminate { core_dump: false },
            Name::Ill => Self::Terminate { core_dump: true },
            Name::Info => Self::Terminate { core_dump: false },
            Name::Int => Self::Terminate { core_dump: false },
            Name::Io => Self::Terminate { core_dump: false },
            Name::Iot => Self::Terminate { core_dump: true },
            Name::Kill => Self::Terminate { core_dump: false },
            Name::Lost => Self::Terminate { core_dump: false },
            Name::Pipe => Self::Terminate { core_dump: false },
            Name::Poll => Self::Terminate { core_dump: false },
            Name::Prof => Self::Terminate { core_dump: false },
            Name::Pwr => Self::Terminate { core_dump: false },
            Name::Quit => Self::Terminate { core_dump: true },
            Name::Segv => Self::Terminate { core_dump: true },
            Name::Stkflt => Self::Terminate { core_dump: false },
            Name::Stop => Self::Suspend,
            Name::Sys => Self::Terminate { core_dump: true },
            Name::Term => Self::Terminate { core_dump: false },
            Name::Thr => Self::Terminate { core_dump: false },
            Name::Trap => Self::Terminate { core_dump: true },
            Name::Tstp | Name::Ttin | Name::Ttou => Self::Suspend,
            Name::Urg => Self::None,
            Name::Usr1 | Name::Usr2 => Self::Terminate { core_dump: false },
            Name::Vtalrm => Self::Terminate { core_dump: false },
            Name::Winch => Self::None,
            Name::Xcpu => Self::Terminate { core_dump: true },
            Name::Xfsz => Self::Terminate { core_dump: true },
            Name::Rtmin(_) | Name::Rtmax(_) => Self::Terminate { core_dump: false },
        }
    }
}
