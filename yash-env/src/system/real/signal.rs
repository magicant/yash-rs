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

//! Signal implementation for the real system

use super::Disposition;
pub use crate::signal::*;
use std::ffi::c_int;
use std::mem::MaybeUninit;
use std::num::NonZeroI32;
use std::ops::RangeInclusive;
use std::ptr::{addr_of, addr_of_mut};

/// Returns the range of real-time signals supported by the real system.
///
/// If the real system does not support real-time signals, this function returns
/// an empty range.
#[must_use]
fn rt_range() -> RangeInclusive<RawNumber> {
    #[cfg(target_os = "aix")]
    return nix::libc::SIGRTMIN..=nix::libc::SIGRTMAX;

    #[cfg(any(
        target_os = "android",
        target_os = "emscripten",
        target_os = "l4re",
        target_os = "linux",
    ))]
    return nix::libc::SIGRTMIN()..=nix::libc::SIGRTMAX();

    #[allow(unreachable_code)]
    {
        #[allow(clippy::reversed_empty_ranges)]
        return 0..=-1;
    }
}

impl Name {
    /// Returns the raw signal number for the real system.
    pub(super) fn to_raw_real(self) -> Option<Number> {
        #[inline]
        fn wrap(number: RawNumber) -> Option<Number> {
            NonZeroI32::new(number).map(Number::from_raw_unchecked)
        }

        match self {
            Self::Abrt => wrap(nix::libc::SIGABRT),
            Self::Alrm => wrap(nix::libc::SIGALRM),
            Self::Bus => wrap(nix::libc::SIGBUS),
            Self::Chld => wrap(nix::libc::SIGCHLD),
            #[cfg(any(
                target_os = "aix",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "solaris",
            ))]
            Self::Cld => wrap(nix::libc::SIGCLD),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "solaris",
            )))]
            Self::Cld => None,
            Self::Cont => wrap(nix::libc::SIGCONT),
            #[cfg(not(any(
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            Self::Emt => wrap(nix::libc::SIGEMT),
            #[cfg(any(
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            ))]
            Self::Emt => None,
            Self::Fpe => wrap(nix::libc::SIGFPE),
            Self::Hup => wrap(nix::libc::SIGHUP),
            Self::Ill => wrap(nix::libc::SIGILL),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            Self::Info => wrap(nix::libc::SIGINFO),
            #[cfg(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            ))]
            Self::Info => None,
            Self::Int => wrap(nix::libc::SIGINT),
            #[cfg(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "linux",
                target_os = "nto",
                target_os = "solaris",
            ))]
            Self::Io => wrap(nix::libc::SIGIO),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "linux",
                target_os = "nto",
                target_os = "solaris",
            )))]
            Self::Io => None,
            Self::Iot => wrap(nix::libc::SIGIOT),
            Self::Kill => wrap(nix::libc::SIGKILL),
            #[cfg(target_os = "horizon")]
            Self::Lost => wrap(nix::libc::SIGLOST),
            #[cfg(not(target_os = "horizon"))]
            Self::Lost => None,
            Self::Pipe => wrap(nix::libc::SIGPIPE),
            #[cfg(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "linux",
                target_os = "nto",
                target_os = "solaris",
            ))]
            Self::Poll => wrap(nix::libc::SIGPOLL),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "linux",
                target_os = "nto",
                target_os = "solaris",
            )))]
            Self::Poll => None,
            Self::Prof => wrap(nix::libc::SIGPROF),
            #[cfg(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "illumos",
                target_os = "linux",
                target_os = "nto",
                target_os = "redox",
                target_os = "solaris",
            ))]
            Self::Pwr => wrap(nix::libc::SIGPWR),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "illumos",
                target_os = "linux",
                target_os = "nto",
                target_os = "redox",
                target_os = "solaris",
            )))]
            Self::Pwr => None,
            Self::Quit => wrap(nix::libc::SIGQUIT),
            Self::Segv => wrap(nix::libc::SIGSEGV),
            #[cfg(all(
                any(
                    target_os = "android",
                    target_os = "emscripten",
                    target_os = "fuchsia",
                    target_os = "linux"
                ),
                not(any(target_arch = "mips", target_arch = "mips64", target_arch = "sparc64"))
            ))]
            Self::Stkflt => wrap(nix::libc::SIGSTKFLT),
            #[cfg(not(all(
                any(
                    target_os = "android",
                    target_os = "emscripten",
                    target_os = "fuchsia",
                    target_os = "linux"
                ),
                not(any(target_arch = "mips", target_arch = "mips64", target_arch = "sparc64"))
            )))]
            Self::Stkflt => None,
            Self::Stop => wrap(nix::libc::SIGSTOP),
            Self::Sys => wrap(nix::libc::SIGSYS),
            Self::Term => wrap(nix::libc::SIGTERM),
            #[cfg(target_os = "freebsd")]
            Self::Thr => wrap(nix::libc::SIGTHR),
            #[cfg(not(target_os = "freebsd"))]
            Self::Thr => None,
            Self::Trap => wrap(nix::libc::SIGTRAP),
            Self::Tstp => wrap(nix::libc::SIGTSTP),
            Self::Ttin => wrap(nix::libc::SIGTTIN),
            Self::Ttou => wrap(nix::libc::SIGTTOU),
            Self::Urg => wrap(nix::libc::SIGURG),
            Self::Usr1 => wrap(nix::libc::SIGUSR1),
            Self::Usr2 => wrap(nix::libc::SIGUSR2),
            Self::Vtalrm => wrap(nix::libc::SIGVTALRM),
            Self::Winch => wrap(nix::libc::SIGWINCH),
            Self::Xcpu => wrap(nix::libc::SIGXCPU),
            Self::Xfsz => wrap(nix::libc::SIGXFSZ),

            Self::Rtmin(n) => {
                let range = rt_range();
                if let Some(number) = range.start().checked_add(n) {
                    if range.contains(&number) {
                        return wrap(number);
                    }
                }
                None
            }
            Self::Rtmax(n) => {
                let range = rt_range();
                if let Some(number) = range.end().checked_add(n) {
                    if range.contains(&number) {
                        return wrap(number);
                    }
                }
                None
            }
        }
    }

    /// Returns the signal for the raw signal number for the real system.
    ///
    /// This function returns `None` if the given number is not a valid signal.
    pub(super) fn try_from_raw_real(number: RawNumber) -> Option<Self> {
        // Some signals share the same number on some systems. This function
        // returns the signal that is considered the most common or standard one.
        #[allow(unreachable_patterns)]
        match number {
            // Standard signals
            nix::libc::SIGABRT => Some(Self::Abrt),
            nix::libc::SIGALRM => Some(Self::Alrm),
            nix::libc::SIGBUS => Some(Self::Bus),
            nix::libc::SIGCHLD => Some(Self::Chld),
            nix::libc::SIGCONT => Some(Self::Cont),
            nix::libc::SIGFPE => Some(Self::Fpe),
            nix::libc::SIGHUP => Some(Self::Hup),
            nix::libc::SIGILL => Some(Self::Ill),
            nix::libc::SIGINT => Some(Self::Int),
            nix::libc::SIGKILL => Some(Self::Kill),
            nix::libc::SIGPIPE => Some(Self::Pipe),
            nix::libc::SIGQUIT => Some(Self::Quit),
            nix::libc::SIGSEGV => Some(Self::Segv),
            nix::libc::SIGSTOP => Some(Self::Stop),
            nix::libc::SIGTERM => Some(Self::Term),
            nix::libc::SIGTSTP => Some(Self::Tstp),
            nix::libc::SIGTTIN => Some(Self::Ttin),
            nix::libc::SIGTTOU => Some(Self::Ttou),
            nix::libc::SIGUSR1 => Some(Self::Usr1),
            nix::libc::SIGUSR2 => Some(Self::Usr2),

            // Non-standard but common signals
            #[cfg(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "linux",
                target_os = "nto",
                target_os = "solaris",
            ))]
            nix::libc::SIGPOLL => Some(Self::Poll),
            nix::libc::SIGPROF => Some(Self::Prof),
            nix::libc::SIGSYS => Some(Self::Sys),
            nix::libc::SIGTRAP => Some(Self::Trap),
            nix::libc::SIGURG => Some(Self::Urg),
            nix::libc::SIGVTALRM => Some(Self::Vtalrm),
            nix::libc::SIGWINCH => Some(Self::Winch),
            nix::libc::SIGXCPU => Some(Self::Xcpu),
            nix::libc::SIGXFSZ => Some(Self::Xfsz),

            // other signals
            #[cfg(not(any(
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            nix::libc::SIGEMT => Some(Self::Emt),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            nix::libc::SIGINFO => Some(Self::Info),
            #[cfg(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "linux",
                target_os = "nto",
                target_os = "solaris",
            ))]
            nix::libc::SIGIO => Some(Self::Io),
            #[cfg(target_os = "horizon")]
            nix::libc::SIGLOST => Some(Self::Lost),
            #[cfg(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "illumos",
                target_os = "linux",
                target_os = "nto",
                target_os = "redox",
                target_os = "solaris",
            ))]
            nix::libc::SIGPWR => Some(Self::Pwr),
            #[cfg(all(
                any(
                    target_os = "android",
                    target_os = "emscripten",
                    target_os = "fuchsia",
                    target_os = "linux"
                ),
                not(any(target_arch = "mips", target_arch = "mips64", target_arch = "sparc64"))
            ))]
            nix::libc::SIGSTKFLT => Some(Self::Stkflt),
            #[cfg(target_os = "freebsd")]
            nix::libc::SIGTHR => Some(Self::Thr),

            // Real-time signals
            _ => {
                let range = rt_range();
                if !range.contains(&number) {
                    return None;
                }

                // Return a name relative to `Rtmin` or `Rtmax`,
                // whichever is closer to the given number.
                debug_assert!(*range.start() > 0);
                let incr = number - *range.start();
                debug_assert!(incr >= 0);
                let decr = number - *range.end();
                debug_assert!(decr <= 0);
                debug_assert!(decr > RawNumber::MIN);
                if incr <= -decr {
                    Some(Self::Rtmin(incr))
                } else {
                    Some(Self::Rtmax(decr))
                }
            }
        }
    }
}

/// Returns an iterator over all signal numbers.
fn all_signals() -> impl Iterator<Item = Number> {
    let non_real_time = Name::iter()
        .filter(|name| !matches!(name, Name::Rtmin(_) | Name::Rtmax(_)))
        .filter_map(Name::to_raw_real);

    let real_time = rt_range()
        .filter_map(NonZeroI32::new)
        .map(Number::from_raw_unchecked);

    non_real_time.chain(real_time)
}

/// Converts the signal set to a vector of signal numbers.
///
/// This function adds the signal numbers in the set to the vector.
pub(super) fn sigset_to_vec(set: *const nix::libc::sigset_t, vec: &mut Vec<Number>) {
    vec.extend(
        all_signals().filter(|number| unsafe { nix::libc::sigismember(set, number.as_raw()) == 1 }),
    );
}

impl Disposition {
    /// Converts the signal disposition to `sigaction` for the real system.
    ///
    /// This function returns the `sigaction` in an `MaybeUninit` because the
    /// `sigaction` structure may contain platform-dependent extra fields that
    /// are not initialized by this function.
    pub(super) fn to_sigaction(self) -> MaybeUninit<nix::libc::sigaction> {
        let handler = match self {
            Disposition::Default => nix::libc::SIG_DFL,
            Disposition::Ignore => nix::libc::SIG_IGN,
            Disposition::Catch => super::catch_signal as *const extern "C" fn(c_int) as _,
        };

        let mut sa = MaybeUninit::<nix::libc::sigaction>::uninit();
        let sa_ptr = sa.as_mut_ptr();
        unsafe {
            addr_of_mut!((*sa_ptr).sa_flags).write(0);
            nix::libc::sigemptyset(addr_of_mut!((*sa_ptr).sa_mask));

            #[cfg(not(target_os = "aix"))]
            #[allow(clippy::useless_transmute)] // See from_sigaction below
            addr_of_mut!((*sa_ptr).sa_sigaction).write(std::mem::transmute(handler));

            #[cfg(target_os = "aix")]
            #[allow(clippy::useless_transmute)] // See from_sigaction below
            addr_of_mut!((*sa_ptr).sa_union.__su_sigaction).write(std::mem::transmute(handler));
        }
        sa
    }

    /// Converts the `sigaction` to the signal disposition for the real system.
    pub(super) unsafe fn from_sigaction(sa: &MaybeUninit<nix::libc::sigaction>) -> Self {
        #[cfg(not(target_os = "aix"))]
        let handler = addr_of!((*sa.as_ptr()).sa_sigaction).read();

        #[cfg(target_os = "aix")]
        let handler = addr_of!((*sa.as_ptr()).sa_union.__su_sigaction).read();

        // It is platform-specific whether we really need to transmute the handler.
        #[allow(clippy::useless_transmute)]
        match std::mem::transmute(handler) {
            nix::libc::SIG_DFL => Self::Default,
            nix::libc::SIG_IGN => Self::Ignore,
            _ => Self::Catch,
        }
    }
}
