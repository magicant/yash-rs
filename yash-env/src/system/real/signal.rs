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

pub use crate::signal::*;
use std::num::NonZeroI32;
use std::ops::RangeInclusive;

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
    pub(super) fn to_raw(self) -> Option<Number> {
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
}
