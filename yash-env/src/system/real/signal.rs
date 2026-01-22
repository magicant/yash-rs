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
use std::num::NonZero;
use std::ops::RangeInclusive;

/// Returns the range of real-time signals supported by the real system.
///
/// If the real system does not support real-time signals, this function returns
/// an empty range.
#[must_use]
pub fn rt_range() -> RangeInclusive<RawNumber> {
    #[cfg(target_os = "aix")]
    return libc::SIGRTMIN..=libc::SIGRTMAX;

    #[cfg(any(
        target_os = "android",
        target_os = "emscripten",
        target_os = "l4re",
        target_os = "linux",
    ))]
    return libc::SIGRTMIN()..=libc::SIGRTMAX();

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
            NonZero::new(number).map(Number::from_raw_unchecked)
        }

        match self {
            Self::Abrt => wrap(libc::SIGABRT),
            Self::Alrm => wrap(libc::SIGALRM),
            Self::Bus => wrap(libc::SIGBUS),
            Self::Chld => wrap(libc::SIGCHLD),
            #[cfg(any(
                target_os = "aix",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "solaris",
            ))]
            Self::Cld => wrap(libc::SIGCLD),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "solaris",
            )))]
            Self::Cld => None,
            Self::Cont => wrap(libc::SIGCONT),
            #[cfg(not(any(
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            Self::Emt => wrap(libc::SIGEMT),
            #[cfg(any(
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            ))]
            Self::Emt => None,
            Self::Fpe => wrap(libc::SIGFPE),
            Self::Hup => wrap(libc::SIGHUP),
            Self::Ill => wrap(libc::SIGILL),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            Self::Info => wrap(libc::SIGINFO),
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
            Self::Int => wrap(libc::SIGINT),
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
            Self::Io => wrap(libc::SIGIO),
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
            Self::Iot => wrap(libc::SIGIOT),
            Self::Kill => wrap(libc::SIGKILL),
            #[cfg(target_os = "horizon")]
            Self::Lost => wrap(libc::SIGLOST),
            #[cfg(not(target_os = "horizon"))]
            Self::Lost => None,
            Self::Pipe => wrap(libc::SIGPIPE),
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
            Self::Poll => wrap(libc::SIGPOLL),
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
            Self::Prof => wrap(libc::SIGPROF),
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
            Self::Pwr => wrap(libc::SIGPWR),
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
            Self::Quit => wrap(libc::SIGQUIT),
            Self::Segv => wrap(libc::SIGSEGV),
            #[cfg(all(
                any(
                    target_os = "android",
                    target_os = "emscripten",
                    target_os = "fuchsia",
                    target_os = "linux"
                ),
                not(any(target_arch = "mips", target_arch = "mips64", target_arch = "sparc64"))
            ))]
            Self::Stkflt => wrap(libc::SIGSTKFLT),
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
            Self::Stop => wrap(libc::SIGSTOP),
            Self::Sys => wrap(libc::SIGSYS),
            Self::Term => wrap(libc::SIGTERM),
            #[cfg(target_os = "freebsd")]
            Self::Thr => wrap(libc::SIGTHR),
            #[cfg(not(target_os = "freebsd"))]
            Self::Thr => None,
            Self::Trap => wrap(libc::SIGTRAP),
            Self::Tstp => wrap(libc::SIGTSTP),
            Self::Ttin => wrap(libc::SIGTTIN),
            Self::Ttou => wrap(libc::SIGTTOU),
            Self::Urg => wrap(libc::SIGURG),
            Self::Usr1 => wrap(libc::SIGUSR1),
            Self::Usr2 => wrap(libc::SIGUSR2),
            Self::Vtalrm => wrap(libc::SIGVTALRM),
            Self::Winch => wrap(libc::SIGWINCH),
            Self::Xcpu => wrap(libc::SIGXCPU),
            Self::Xfsz => wrap(libc::SIGXFSZ),

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
            libc::SIGABRT => Some(Self::Abrt),
            libc::SIGALRM => Some(Self::Alrm),
            libc::SIGBUS => Some(Self::Bus),
            libc::SIGCHLD => Some(Self::Chld),
            libc::SIGCONT => Some(Self::Cont),
            libc::SIGFPE => Some(Self::Fpe),
            libc::SIGHUP => Some(Self::Hup),
            libc::SIGILL => Some(Self::Ill),
            libc::SIGINT => Some(Self::Int),
            libc::SIGKILL => Some(Self::Kill),
            libc::SIGPIPE => Some(Self::Pipe),
            libc::SIGQUIT => Some(Self::Quit),
            libc::SIGSEGV => Some(Self::Segv),
            libc::SIGSTOP => Some(Self::Stop),
            libc::SIGTERM => Some(Self::Term),
            libc::SIGTSTP => Some(Self::Tstp),
            libc::SIGTTIN => Some(Self::Ttin),
            libc::SIGTTOU => Some(Self::Ttou),
            libc::SIGUSR1 => Some(Self::Usr1),
            libc::SIGUSR2 => Some(Self::Usr2),

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
            libc::SIGPOLL => Some(Self::Poll),
            libc::SIGPROF => Some(Self::Prof),
            libc::SIGSYS => Some(Self::Sys),
            libc::SIGTRAP => Some(Self::Trap),
            libc::SIGURG => Some(Self::Urg),
            libc::SIGVTALRM => Some(Self::Vtalrm),
            libc::SIGWINCH => Some(Self::Winch),
            libc::SIGXCPU => Some(Self::Xcpu),
            libc::SIGXFSZ => Some(Self::Xfsz),

            // other signals
            #[cfg(not(any(
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            libc::SIGEMT => Some(Self::Emt),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            libc::SIGINFO => Some(Self::Info),
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
            libc::SIGIO => Some(Self::Io),
            #[cfg(target_os = "horizon")]
            libc::SIGLOST => Some(Self::Lost),
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
            libc::SIGPWR => Some(Self::Pwr),
            #[cfg(all(
                any(
                    target_os = "android",
                    target_os = "emscripten",
                    target_os = "fuchsia",
                    target_os = "linux"
                ),
                not(any(target_arch = "mips", target_arch = "mips64", target_arch = "sparc64"))
            ))]
            libc::SIGSTKFLT => Some(Self::Stkflt),
            #[cfg(target_os = "freebsd")]
            libc::SIGTHR => Some(Self::Thr),

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
        .filter_map(NonZero::new)
        .map(Number::from_raw_unchecked);

    non_real_time.chain(real_time)
}

/// Converts the signal set to a vector of signal numbers.
///
/// This function adds the signal numbers in the set to the vector.
pub(super) fn sigset_to_vec(set: *const libc::sigset_t, vec: &mut Vec<Number>) {
    vec.extend(
        all_signals().filter(|number| unsafe { libc::sigismember(set, number.as_raw()) == 1 }),
    );
}

impl Disposition {
    /// Converts the signal disposition to `sigaction` for the real system.
    ///
    /// This function returns the `sigaction` in an `MaybeUninit` because the
    /// `sigaction` structure may contain platform-dependent extra fields that
    /// are not initialized by this function.
    pub(super) fn to_sigaction(self) -> MaybeUninit<libc::sigaction> {
        let handler = match self {
            Disposition::Default => libc::SIG_DFL,
            Disposition::Ignore => libc::SIG_IGN,
            Disposition::Catch => super::catch_signal as *const extern "C" fn(c_int) as _,
        };

        let mut sa = MaybeUninit::<libc::sigaction>::uninit();
        let sa_ptr = sa.as_mut_ptr();
        unsafe {
            (&raw mut (*sa_ptr).sa_flags).write(0);
            libc::sigemptyset(&raw mut (*sa_ptr).sa_mask);

            #[cfg(not(target_os = "aix"))]
            #[allow(clippy::useless_transmute)] // See from_sigaction below
            (&raw mut (*sa_ptr).sa_sigaction).write(std::mem::transmute(handler));

            #[cfg(target_os = "aix")]
            #[allow(clippy::useless_transmute)] // See from_sigaction below
            (&raw mut (*sa_ptr).sa_union.__su_sigaction).write(std::mem::transmute(handler));
        }
        sa
    }

    /// Converts the `sigaction` to the signal disposition for the real system.
    pub(super) unsafe fn from_sigaction(sa: &MaybeUninit<libc::sigaction>) -> Self {
        unsafe {
            #[cfg(not(target_os = "aix"))]
            let handler = (*sa.as_ptr()).sa_sigaction;

            #[cfg(target_os = "aix")]
            let handler = (*sa.as_ptr()).sa_union.__su_sigaction;

            // It is platform-specific whether we really need to transmute the handler.
            #[allow(clippy::useless_transmute)]
            match std::mem::transmute(handler) {
                libc::SIG_DFL => Self::Default,
                libc::SIG_IGN => Self::Ignore,
                _ => Self::Catch,
            }
        }
    }
}
