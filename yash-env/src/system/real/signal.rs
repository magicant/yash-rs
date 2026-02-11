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
use super::ErrnoIfM1 as _;
use super::Result;
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

/// Signal set for the real system
///
/// This is an implementation of the [`Sigset` trait](super::super::Sigset) for the
/// [`RealSystem`](super::RealSystem).
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct Sigset(pub(super) MaybeUninit<libc::sigset_t>);

impl Default for Sigset {
    fn default() -> Self {
        let mut set = MaybeUninit::<libc::sigset_t>::uninit();
        unsafe { libc::sigemptyset(set.as_mut_ptr()) }
            .errno_if_m1()
            .expect("sigemptyset failed");
        Self(set)
    }
}

impl super::super::Sigset for Sigset {
    fn add(&mut self, signal: Number) -> Result<()> {
        unsafe { libc::sigaddset(self.0.as_mut_ptr(), signal.as_raw()) }
            .errno_if_m1()
            .map(drop)
    }

    fn remove(&mut self, signal: Number) -> Result<()> {
        unsafe { libc::sigdelset(self.0.as_mut_ptr(), signal.as_raw()) }
            .errno_if_m1()
            .map(drop)
    }

    fn contains(&self, signal: Number) -> Result<bool> {
        unsafe { libc::sigismember(self.0.as_ptr(), signal.as_raw()) }
            .errno_if_m1()
            .map(|result| result != 0)
    }
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
