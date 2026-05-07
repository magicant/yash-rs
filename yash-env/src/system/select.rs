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
use super::signal;
use crate::io::Fd;
use crate::system::Signals;
use std::ffi::c_int;
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

/// Trait for performing the `select` operation
pub trait Select: Signals {
    /// Waits for a next event.
    ///
    /// In a typical configuration, this trait is not used directly. Instead,
    /// it is used by [`Concurrent`] to implement asynchronous I/O, signal
    /// handling, and timer functions.
    ///
    /// This function blocks the calling thread until one of the following
    /// condition is met:
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
        signal_mask: Option<&[signal::Number]>,
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
        signal_mask: Option<&[signal::Number]>,
    ) -> impl Future<Output = Result<c_int>> + use<'a, S> {
        (self as &S).select(readers, writers, timeout, signal_mask)
    }
}
