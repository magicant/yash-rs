// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
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

//! Implementation of file descriptor sets for use with `select` system call

use crate::io::Fd;
use crate::system::FdSet as FdSetTrait;
use std::mem::MaybeUninit;
use std::os::fd::RawFd;

/// File descriptor set for the real system, wrapping the `fd_set` type from libc
///
/// This is an implementation of the [`FdSet` trait](crate::system::FdSet) for the
/// [`RealSystem`](super::RealSystem).
#[derive(Clone)]
pub struct FdSet {
    /// The underlying `fd_set` structure from libc
    ///
    /// We use `MaybeUninit` because `fd_set` may contain uninitialized fields
    /// or padding bytes that are not initialized by `FD_ZERO`.
    fds: MaybeUninit<libc::fd_set>,

    /// An FD that is greater than any FD currently in the set
    ///
    /// This is used to optimize the `select` system call by providing an upper bound
    /// on the FDs in the set. The `select` system call only needs to check FDs
    /// up to this upper bound, which can improve performance when the set
    /// contains a small number of FDs that are much smaller than `FD_SETSIZE`.
    upper_bound: Fd,
}

impl Default for FdSet {
    fn default() -> Self {
        let mut fds = MaybeUninit::<libc::fd_set>::uninit();
        unsafe { libc::FD_ZERO(fds.as_mut_ptr()) };
        Self {
            fds,
            upper_bound: Fd(0),
        }
    }
}

impl FdSetTrait for FdSet {
    const MAX_FD: Fd = Fd(libc::FD_SETSIZE as RawFd - 1);

    fn insert(&mut self, fd: Fd) {
        if 0 <= fd.0 && fd <= Self::MAX_FD {
            // SAFETY: We just verified that `fd` is within the valid range for
            // an `fd_set`.
            // SAFETY: POSIX also requires that `fd` be valid (i.e. open), but
            // we cannot verify that here. We assume that the underlying
            // `fd_set` implementation accepts invalid FDs and the `select`
            // system call will handle them appropriately.
            unsafe { libc::FD_SET(fd.0, self.fds.as_mut_ptr()) };

            self.upper_bound = self.upper_bound.max(Fd(fd.0 + 1));
        }
    }

    fn remove(&mut self, fd: Fd) {
        if 0 <= fd.0 && fd <= Self::MAX_FD {
            // SAFETY: The same as in `insert`.
            unsafe { libc::FD_CLR(fd.0, self.fds.as_mut_ptr()) };
        }
    }

    fn contains(&self, fd: Fd) -> bool {
        0 <= fd.0 && fd <= Self::MAX_FD && unsafe { libc::FD_ISSET(fd.0, self.fds.as_ptr()) }
    }
}

impl std::fmt::Debug for FdSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fds = (0..=Self::MAX_FD.0).filter(|&fd| self.contains(Fd(fd)));
        f.debug_set().entries(fds).finish()
    }
}

impl FdSet {
    #[inline(always)]
    #[must_use]
    pub(super) fn as_mut_ptr(&mut self) -> *mut libc::fd_set {
        self.fds.as_mut_ptr()
    }

    #[inline(always)]
    #[must_use]
    pub(super) fn upper_bound(&self) -> Fd {
        self.upper_bound
    }
}
