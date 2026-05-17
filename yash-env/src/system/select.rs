// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Items related to the `select` system call

#[cfg(doc)]
use super::Concurrent;
use super::Result;
use super::Sigmask;
use crate::io::Fd;
use std::ffi::c_int;
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

/// Trait for representing a set of file descriptors (FDs)
///
/// This is an abstraction over the `fd_set` type used in the `select` system
/// call. It represents a set of [`Fd`]s that can be monitored for events such
/// as readability or writability.
///
/// As per POSIX, an `fd_set` can only contain FDs in the range of `0` to
/// `FD_SETSIZE - 1`. The [`MAX_FD`](Self::MAX_FD) associated constant in this
/// trait represents the maximum FD that can be stored in the set.
pub trait FdSet: Clone + Default + 'static {
    /// The maximum FD that can be stored in the set. This corresponds to
    /// `FD_SETSIZE - 1` in C libraries. The exact value may depend on the
    /// implementation.
    const MAX_FD: Fd;

    /// Creates a new, empty FD set.
    ///
    /// The provided implementation simply calls [`Default::default`].
    #[inline(always)]
    #[must_use]
    fn new() -> Self {
        Default::default()
    }

    /// Adds an FD to the set.
    ///
    /// If the provided FD is greater than [`MAX_FD`](Self::MAX_FD) or negative,
    /// it will be silently ignored.
    fn insert(&mut self, fd: Fd);

    /// Removes an FD from the set.
    ///
    /// If the provided FD is greater than [`MAX_FD`](Self::MAX_FD) or negative,
    /// it will be silently ignored.
    fn remove(&mut self, fd: Fd);

    /// Checks if an FD is in the set.
    ///
    /// If the provided FD is greater than [`MAX_FD`](Self::MAX_FD) or negative,
    /// this function will return `false`.
    fn contains(&self, fd: Fd) -> bool;
}

/// Trait for performing the `select` operation
///
/// This trait provides the `select` method, which represents the `select`
/// system call. This trait is different from the
/// [same-named trait in the `concurrency` submodule](super::concurrency::Select),
/// which provides a higher-level interface for waiting on multiple events. The
/// `Select` trait in this module is a lower-level interface that directly
/// represents the behavior of the `select` system call. It is used by the
/// `Select` trait in the `concurrency` submodule to implement its
/// functionality.
pub trait Select: Sigmask {
    /// Waits for a next event.
    ///
    /// In a typical configuration, this trait is not used directly. Instead,
    /// it is used by [`Concurrent`] to implement asynchronous I/O, signal
    /// handling, and timer functions.
    ///
    /// This function blocks the calling thread until one of the following
    /// conditions is met:
    ///
    /// - An FD in `readers` becomes ready for reading.
    /// - An FD in `writers` becomes ready for writing.
    /// - The specified `timeout` duration has passed.
    /// - A signal handler catches a signal.
    ///
    /// When this function returns an `Ok`, FDs that are not ready for reading
    /// and writing are removed from `readers` and `writers`, respectively. The
    /// return value will be the number of FDs left in `readers` and `writers`.
    ///
    /// If `readers` and `writers` contain an FD that is not open for reading
    /// and writing, respectively, this function will fail with `EBADF`. In this
    /// case, you should remove the FD from `readers` and `writers` and try
    /// again.
    ///
    /// If `signal_mask` is `Some` list of signals, it is used as the signal
    /// blocking mask while waiting and restored when the function returns.
    ///
    /// The return type is a future so that
    /// [virtual systems](crate::system::virtual) can simulate the blocking
    /// behavior of `select` without blocking the entire process. The future
    /// will be ready when one of the above conditions is met. The future may
    /// also return `Pending` if the virtual process is suspended by a signal.
    /// In a [real system](super::real), this function does not work
    /// asynchronously and returns a ready `Future` with the result of the
    /// underlying system call. See the [module-level documentation](super) for
    /// details.
    fn select<'a>(
        &self,
        readers: &'a mut Vec<Fd>,
        writers: &'a mut Vec<Fd>,
        timeout: Option<Duration>,
        signal_mask: Option<&Self::Sigset>,
    ) -> impl Future<Output = Result<c_int>> + use<'a, Self>;
}

/// Delegates the `Select` trait to the contained instance of `S`
impl<S: Select> Select for Rc<S> {
    #[inline]
    fn select<'a>(
        &self,
        readers: &'a mut Vec<Fd>,
        writers: &'a mut Vec<Fd>,
        timeout: Option<Duration>,
        signal_mask: Option<&S::Sigset>,
    ) -> impl Future<Output = Result<c_int>> + use<'a, S> {
        (self as &S).select(readers, writers, timeout, signal_mask)
    }
}
