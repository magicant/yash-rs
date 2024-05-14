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

//! Signal definitions for the real system

use crate::trap::Signal2;
use crate::trap::UnknownSignalError;
use std::ffi::c_int;
use std::ops::RangeInclusive;

/// Returns the range of real-time signals supported by the real system.
///
/// If the real system does not support real-time signals, `None` is returned.
#[must_use]
fn rt_range() -> Option<RangeInclusive<c_int>> {
    #[cfg(target_os = "aix")]
    return Some(nix::libc::SIGRTMIN..=nix::libc::SIGRTMAX);

    #[cfg(any(
        target_os = "android",
        target_os = "emscripten",
        target_os = "l4re",
        target_os = "linux",
    ))]
    return Some(nix::libc::SIGRTMIN()..=nix::libc::SIGRTMAX());

    #[allow(unreachable_code)]
    None
}

impl Signal2 {
    /// Returns the raw signal number for the real system.
    pub(super) fn to_raw(self) -> Result<c_int, UnknownSignalError> {
        match self {
            Signal2::Abrt => Ok(nix::libc::SIGABRT),
            Signal2::Alrm => Ok(nix::libc::SIGALRM),
            Signal2::Bus => Ok(nix::libc::SIGBUS),
            Signal2::Chld => Ok(nix::libc::SIGCHLD),
            #[cfg(any(
                target_os = "aix",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "solaris",
            ))]
            Signal2::Cld => Ok(nix::libc::SIGCLD),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "horizon",
                target_os = "illumos",
                target_os = "solaris",
            )))]
            Signal2::Cld => Err(UnknownSignalError),
            Signal2::Cont => Ok(nix::libc::SIGCONT),
            #[cfg(not(any(
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            Signal2::Emt => Ok(nix::libc::SIGEMT),
            #[cfg(any(
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            ))]
            Signal2::Emt => Err(UnknownSignalError),
            Signal2::Fpe => Ok(nix::libc::SIGFPE),
            Signal2::Hup => Ok(nix::libc::SIGHUP),
            Signal2::Ill => Ok(nix::libc::SIGILL),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            Signal2::Info => Ok(nix::libc::SIGINFO),
            #[cfg(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            ))]
            Signal2::Info => Err(UnknownSignalError),
            Signal2::Int => Ok(nix::libc::SIGINT),
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
            Signal2::Io => Ok(nix::libc::SIGIO),
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
            Signal2::Io => Err(UnknownSignalError),
            Signal2::Iot => Ok(nix::libc::SIGIOT),
            Signal2::Kill => Ok(nix::libc::SIGKILL),
            #[cfg(target_os = "horizon")]
            Signal2::Lost => Ok(nix::libc::SIGLOST),
            #[cfg(not(target_os = "horizon"))]
            Signal2::Lost => Err(UnknownSignalError),
            Signal2::Pipe => Ok(nix::libc::SIGPIPE),
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
            Signal2::Poll => Ok(nix::libc::SIGPOLL),
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
            Signal2::Poll => Err(UnknownSignalError),
            Signal2::Prof => Ok(nix::libc::SIGPROF),
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
            Signal2::Pwr => Ok(nix::libc::SIGPWR),
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
            Signal2::Pwr => Err(UnknownSignalError),
            Signal2::Quit => Ok(nix::libc::SIGQUIT),
            Signal2::Segv => Ok(nix::libc::SIGSEGV),
            #[cfg(all(
                any(
                    target_os = "android",
                    target_os = "emscripten",
                    target_os = "fuchsia",
                    target_os = "linux"
                ),
                not(any(target_arch = "mips", target_arch = "mips64", target_arch = "sparc64"))
            ))]
            Signal2::Stkflt => Ok(nix::libc::SIGSTKFLT),
            #[cfg(not(all(
                any(
                    target_os = "android",
                    target_os = "emscripten",
                    target_os = "fuchsia",
                    target_os = "linux"
                ),
                not(any(target_arch = "mips", target_arch = "mips64", target_arch = "sparc64"))
            )))]
            Signal2::Stkflt => Err(UnknownSignalError),
            Signal2::Stop => Ok(nix::libc::SIGSTOP),
            Signal2::Sys => Ok(nix::libc::SIGSYS),
            Signal2::Term => Ok(nix::libc::SIGTERM),
            #[cfg(target_os = "freebsd")]
            Signal2::Thr => Ok(nix::libc::SIGTHR),
            #[cfg(not(target_os = "freebsd"))]
            Signal2::Thr => Err(UnknownSignalError),
            Signal2::Trap => Ok(nix::libc::SIGTRAP),
            Signal2::Tstp => Ok(nix::libc::SIGTSTP),
            Signal2::Ttin => Ok(nix::libc::SIGTTIN),
            Signal2::Ttou => Ok(nix::libc::SIGTTOU),
            Signal2::Urg => Ok(nix::libc::SIGURG),
            Signal2::Usr1 => Ok(nix::libc::SIGUSR1),
            Signal2::Usr2 => Ok(nix::libc::SIGUSR2),
            Signal2::Vtalrm => Ok(nix::libc::SIGVTALRM),
            Signal2::Winch => Ok(nix::libc::SIGWINCH),
            Signal2::Xcpu => Ok(nix::libc::SIGXCPU),
            Signal2::Xfsz => Ok(nix::libc::SIGXFSZ),

            Signal2::Rtmin(n) => {
                if let Some(range) = rt_range() {
                    if let Some(number) = range.start().checked_add(n) {
                        if range.contains(&number) {
                            return Ok(number);
                        }
                    }
                }
                Err(UnknownSignalError)
            }
            Signal2::Rtmax(n) => {
                if let Some(range) = rt_range() {
                    if let Some(number) = range.end().checked_add(n) {
                        if range.contains(&number) {
                            return Ok(number);
                        }
                    }
                }
                Err(UnknownSignalError)
            }

            Signal2::Number(n) => {
                // Check if the number is valid
                if Self::try_from_raw(n).is_ok() {
                    Ok(n)
                } else {
                    Err(UnknownSignalError)
                }
            }
        }
    }

    /// Returns the signal for the raw signal number for the real system.
    ///
    /// This function returns `UnknownSignalError` if the given number is not a
    /// valid signal.
    pub(super) fn try_from_raw(number: c_int) -> Result<Self, UnknownSignalError> {
        // Some signals share the same number on some systems. This function
        // returns the signal that is considered the most common or standard one.
        #[allow(unreachable_patterns)]
        match number {
            // Standard signals
            nix::libc::SIGABRT => Ok(Signal2::Abrt),
            nix::libc::SIGALRM => Ok(Signal2::Alrm),
            nix::libc::SIGBUS => Ok(Signal2::Bus),
            nix::libc::SIGCHLD => Ok(Signal2::Chld),
            nix::libc::SIGCONT => Ok(Signal2::Cont),
            nix::libc::SIGFPE => Ok(Signal2::Fpe),
            nix::libc::SIGHUP => Ok(Signal2::Hup),
            nix::libc::SIGILL => Ok(Signal2::Ill),
            nix::libc::SIGINT => Ok(Signal2::Int),
            nix::libc::SIGKILL => Ok(Signal2::Kill),
            nix::libc::SIGPIPE => Ok(Signal2::Pipe),
            nix::libc::SIGQUIT => Ok(Signal2::Quit),
            nix::libc::SIGSEGV => Ok(Signal2::Segv),
            nix::libc::SIGSTOP => Ok(Signal2::Stop),
            nix::libc::SIGTERM => Ok(Signal2::Term),
            nix::libc::SIGTSTP => Ok(Signal2::Tstp),
            nix::libc::SIGTTIN => Ok(Signal2::Ttin),
            nix::libc::SIGTTOU => Ok(Signal2::Ttou),
            nix::libc::SIGUSR1 => Ok(Signal2::Usr1),
            nix::libc::SIGUSR2 => Ok(Signal2::Usr2),

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
            nix::libc::SIGPOLL => Ok(Signal2::Poll),
            nix::libc::SIGPROF => Ok(Signal2::Prof),
            nix::libc::SIGSYS => Ok(Signal2::Sys),
            nix::libc::SIGTRAP => Ok(Signal2::Trap),
            nix::libc::SIGURG => Ok(Signal2::Urg),
            nix::libc::SIGVTALRM => Ok(Signal2::Vtalrm),
            nix::libc::SIGWINCH => Ok(Signal2::Winch),
            nix::libc::SIGXCPU => Ok(Signal2::Xcpu),
            nix::libc::SIGXFSZ => Ok(Signal2::Xfsz),

            // other signals
            #[cfg(not(any(
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            nix::libc::SIGEMT => Ok(Signal2::Emt),
            #[cfg(not(any(
                target_os = "aix",
                target_os = "android",
                target_os = "emscripten",
                target_os = "fuchsia",
                target_os = "haiku",
                target_os = "linux",
                target_os = "redox",
            )))]
            nix::libc::SIGINFO => Ok(Signal2::Info),
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
            nix::libc::SIGIO => Ok(Signal2::Io),
            #[cfg(target_os = "horizon")]
            nix::libc::SIGLOST => Ok(Signal2::Lost),
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
            nix::libc::SIGPWR => Ok(Signal2::Pwr),
            #[cfg(all(
                any(
                    target_os = "android",
                    target_os = "emscripten",
                    target_os = "fuchsia",
                    target_os = "linux"
                ),
                not(any(target_arch = "mips", target_arch = "mips64", target_arch = "sparc64"))
            ))]
            nix::libc::SIGSTKFLT => Ok(Signal2::Stkflt),
            #[cfg(target_os = "freebsd")]
            nix::libc::SIGTHR => Ok(Signal2::Thr),

            _ => {
                // Real-time signals
                if let Some(range) = rt_range() {
                    if range.contains(&number) {
                        let incr = number - range.start();
                        debug_assert!(incr >= 0);
                        let decr = number - range.end();
                        debug_assert!(decr <= 0);
                        return if incr <= -decr {
                            Ok(Signal2::Rtmin(incr))
                        } else {
                            Ok(Signal2::Rtmax(decr))
                        };
                    }
                }

                Err(UnknownSignalError)
            }
        }
    }
}
