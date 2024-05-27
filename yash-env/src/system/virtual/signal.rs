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

use super::super::Signal;
pub(super) use crate::signal::*;
use std::num::NonZeroI32;

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
    #[must_use]
    pub fn of(signal: Signal) -> Self {
        match signal {
            Signal::SIGHUP => Self::Terminate { core_dump: false },
            Signal::SIGINT => Self::Terminate { core_dump: false },
            Signal::SIGQUIT => Self::Terminate { core_dump: true },
            Signal::SIGILL => Self::Terminate { core_dump: true },
            Signal::SIGTRAP => Self::Terminate { core_dump: true },
            Signal::SIGABRT => Self::Terminate { core_dump: true },
            Signal::SIGBUS => Self::Terminate { core_dump: true },
            // Signal::SIGEMT => Self::Terminate { core_dump: false },
            Signal::SIGFPE => Self::Terminate { core_dump: true },
            Signal::SIGKILL => Self::Terminate { core_dump: false },
            Signal::SIGUSR1 => Self::Terminate { core_dump: false },
            Signal::SIGSEGV => Self::Terminate { core_dump: true },
            Signal::SIGUSR2 => Self::Terminate { core_dump: false },
            Signal::SIGPIPE => Self::Terminate { core_dump: false },
            Signal::SIGALRM => Self::Terminate { core_dump: false },
            Signal::SIGTERM => Self::Terminate { core_dump: false },
            // Signal::SIGSTKFLT => Self::Terminate { core_dump: false },
            Signal::SIGCHLD => Self::None,
            Signal::SIGCONT => Self::Resume,
            Signal::SIGSTOP => Self::Suspend,
            Signal::SIGTSTP => Self::Suspend,
            Signal::SIGTTIN => Self::Suspend,
            Signal::SIGTTOU => Self::Suspend,
            Signal::SIGURG => Self::None,
            Signal::SIGXCPU => Self::Terminate { core_dump: true },
            Signal::SIGXFSZ => Self::Terminate { core_dump: true },
            Signal::SIGVTALRM => Self::Terminate { core_dump: false },
            Signal::SIGPROF => Self::Terminate { core_dump: false },
            Signal::SIGWINCH => Self::None,
            Signal::SIGIO => Self::Terminate { core_dump: false },
            // Signal::SIGPWR => Self::Terminate { core_dump: false },
            // Signal::SIGINFO => Self::Terminate { core_dump: false },
            Signal::SIGSYS => Self::Terminate { core_dump: true },
            _ => Self::Terminate { core_dump: false },
        }
    }
}

/// Signal number for `SIGABRT` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGABRT` is 6.
pub const SIGABRT: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(6) });

/// Signal number for `SIGALRM` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGALRM` is 14.
pub const SIGALRM: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(14) });

/// Signal number for `SIGBUS` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGBUS: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(101) });

/// Signal number for `SIGCHLD` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGCHLD: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(102) });

/// Signal number for `SIGCLD` in the virtual system
///
/// Currently, this is the same as `SIGCHLD`.
pub const SIGCLD: Number = SIGCHLD;

/// Signal number for `SIGCONT` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGCONT: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(103) });

/// Signal number for `SIGEMT` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGEMT: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(104) });

/// Signal number for `SIGFPE` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGFPE: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(105) });

/// Signal number for `SIGHUP` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGHUP` is 1.
pub const SIGHUP: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(1) });

/// Signal number for `SIGILL` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGILL: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(106) });

/// Signal number for `SIGINFO` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGINFO: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(107) });

/// Signal number for `SIGINT` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGINT` is 2.
pub const SIGINT: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(2) });

/// Signal number for `SIGIO` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGIO: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(108) });

/// Signal number for `SIGIOT` in the virtual system
///
/// Currently, this is the same as `SIGABRT`.
pub const SIGIOT: Number = SIGABRT;

/// Signal number for `SIGKILL` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGKILL` is 9.
pub const SIGKILL: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(9) });

/// Signal number for `SIGLOST` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGLOST: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(109) });

/// Signal number for `SIGPIPE` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGPIPE: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(110) });

/// Signal number for `SIGPOLL` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGPOLL: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(111) });

/// Signal number for `SIGPROF` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGPROF: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(112) });

/// Signal number for `SIGPWR` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGPWR: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(113) });

/// Signal number for `SIGQUIT` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGQUIT` is 3.
pub const SIGQUIT: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(3) });

/// Signal number for `SIGSEGV` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGSEGV: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(114) });

/// Signal number for `SIGSTKFLT` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGSTKFLT: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(115) });

/// Signal number for `SIGSTOP` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGSTOP: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(116) });

/// Signal number for `SIGSYS` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGSYS: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(117) });

/// Signal number for `SIGTERM` in the virtual system
///
/// POSIX effectively requires that the signal number for `SIGTERM` is 15.
pub const SIGTERM: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(15) });

/// Signal number for `SIGTHR` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGTHR: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(118) });

/// Signal number for `SIGTRAP` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGTRAP: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(119) });

/// Signal number for `SIGTSTP` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGTSTP: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(120) });

/// Signal number for `SIGTTIN` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGTTIN: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(121) });

/// Signal number for `SIGTTOU` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGTTOU: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(122) });

/// Signal number for `SIGURG` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGURG: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(123) });

/// Signal number for `SIGUSR1` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGUSR1: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(124) });

/// Signal number for `SIGUSR2` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGUSR2: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(125) });

/// Signal number for `SIGVTALRM` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGVTALRM: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(126) });

/// Signal number for `SIGWINCH` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGWINCH: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(127) });

/// Signal number for `SIGXCPU` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGXCPU: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(128) });

/// Signal number for `SIGXFSZ` in the virtual system
///
/// Note that this is not the same as the signal number in the real system.
pub const SIGXFSZ: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(129) });

/// Signal number for `SIGRTMIN` in the virtual system
///
/// The current implementation supports nine real-time signals (201 through 209).
pub const SIGRTMIN: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(201) });

/// Signal number for `SIGRTMAX` in the virtual system
///
/// The current implementation supports nine real-time signals (201 through 209).
pub const SIGRTMAX: Number = Number::from_raw_unchecked(unsafe { NonZeroI32::new_unchecked(209) });

/// Range of the real-time signals supported by the virtual system.
const RT_RANGE: std::ops::RangeInclusive<RawNumber> = SIGRTMIN.as_raw()..=SIGRTMAX.as_raw();

impl Name {
    pub(super) fn to_raw_virtual(self) -> Option<Number> {
        fn rt(base: Number, n: RawNumber) -> Option<Number> {
            let number = base.as_raw().checked_add(n)?;
            let non_zero = NonZeroI32::new(number)?;
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

// TODO Remove this
impl Number {
    /// Converts a signal number in the real system to a signal number in the virtual system.
    pub(super) fn from_signal_virtual(signal: Signal) -> Self {
        use crate::system::System as _;
        unsafe { crate::RealSystem::new() }
            .validate_signal(signal as RawNumber)
            .and_then(|(name, _real_number)| name.to_raw_virtual())
            .unwrap()
    }

    /// Converts a signal number in the virtual system to a signal number in the real system.
    pub(super) fn to_signal_virtual(self) -> Option<Signal> {
        use crate::system::System as _;
        unsafe { crate::RealSystem::new() }
            .signal_number_from_name(Name::try_from_raw_virtual(self.as_raw())?)?
            .as_raw()
            .try_into()
            .ok()
    }
}
