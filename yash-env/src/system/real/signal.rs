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

use super::super::Result;
use super::Disposition;
use super::ErrnoIfM1 as _;
pub use crate::signal::*;
use std::ffi::c_int;
use std::mem::MaybeUninit;
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

    #[allow(unreachable_code, reason = "for readability")] // TODO: use cfg_select
    #[allow(clippy::reversed_empty_ranges, reason = "false positive")]
    return 0..=-1;
}

/// Signal set for the real system, wrapping the `sigset_t` type from libc
///
/// This is an implementation of the [`Sigset` trait](super::super::Sigset) for the
/// [`RealSystem`](super::RealSystem).
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct Sigset(pub(super) MaybeUninit<libc::sigset_t>);
// TODO: The auto-derived Debug implementation does not provide useful information.
// Consider implementing a custom Debug that shows the contents.

impl Sigset {
    /// Converts a raw `sigset_t` structure to a `Sigset` object.
    ///
    /// This function assumes the `sigset_t` structure to be initialized by the
    /// `sigemptyset` or `sigfillset` calls, but it is passed as `MaybeUninit`
    /// because of possible padding or extension fields in the structure which
    /// may not be initialized by those calls.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the provided `sigset_t` structure is properly
    /// initialized by a call like `sigemptyset` or `sigfillset`.
    pub(super) const unsafe fn from_raw(sigset: MaybeUninit<libc::sigset_t>) -> Self {
        Self(sigset)
    }
}

impl Default for Sigset {
    /// Creates an empty signal set.
    fn default() -> Self {
        let mut sigset = MaybeUninit::<libc::sigset_t>::uninit();
        unsafe {
            libc::sigemptyset(sigset.as_mut_ptr())
                .errno_if_m1()
                .expect("sigemptyset should always succeed");
            Self::from_raw(sigset)
        }
    }
}

impl super::super::Sigset for Sigset {
    fn full() -> Self {
        let mut sigset = MaybeUninit::<libc::sigset_t>::uninit();
        unsafe {
            libc::sigfillset(sigset.as_mut_ptr())
                .errno_if_m1()
                .expect("sigfillset should always succeed");
            Self::from_raw(sigset)
        }
    }

    fn insert(&mut self, signal: Number) -> Result<()> {
        let result = unsafe { libc::sigaddset(self.0.as_mut_ptr(), signal.as_raw()) };
        result.errno_if_m1().map(drop)
    }

    fn remove(&mut self, signal: Number) -> Result<()> {
        let result = unsafe { libc::sigdelset(self.0.as_mut_ptr(), signal.as_raw()) };
        result.errno_if_m1().map(drop)
    }

    fn contains(&self, signal: Number) -> Result<bool> {
        let result = unsafe { libc::sigismember(self.0.as_ptr(), signal.as_raw()) };
        result.errno_if_m1().map(|r| r > 0)
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

            // TODO: use cfg_select
            #[cfg(not(target_os = "aix"))]
            #[allow(
                clippy::useless_transmute,
                reason = "the type of sa_sigaction may vary across platforms"
            )]
            (&raw mut (*sa_ptr).sa_sigaction).write(std::mem::transmute(handler));

            #[cfg(target_os = "aix")]
            #[allow(
                clippy::useless_transmute,
                reason = "the type of __su_sigaction may vary across platforms"
            )]
            (&raw mut (*sa_ptr).sa_union.__su_sigaction).write(std::mem::transmute(handler));
        }
        sa
    }

    /// Converts the `sigaction` to the signal disposition for the real system.
    pub(super) unsafe fn from_sigaction(sa: &MaybeUninit<libc::sigaction>) -> Self {
        unsafe {
            // TODO: use cfg_select
            #[cfg(not(target_os = "aix"))]
            let handler = (*sa.as_ptr()).sa_sigaction;

            #[cfg(target_os = "aix")]
            let handler = (*sa.as_ptr()).sa_union.__su_sigaction;

            #[allow(
                clippy::useless_transmute,
                reason = "the type of handler may vary across platforms"
            )]
            match std::mem::transmute(handler) {
                libc::SIG_DFL => Self::Default,
                libc::SIG_IGN => Self::Ignore,
                _ => Self::Catch,
            }
        }
    }
}
