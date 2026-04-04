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

//! Items for concurrent task execution

use super::{Read, Result, Select, Write, signal};
use crate::io::Fd;
use std::ffi::c_int;
use std::rc::Rc;

/// Decorator for systems that makes blocking I/O operations concurrency-friendly
///
/// This struct is used as a wrapper for systems for enabling concurrent
/// execution of multiple possibly blocking I/O tasks on a single thread. The
/// inner system is expected to implement the [`Read`], [`Write`], and
/// [`Select`] traits with synchronous (blocking) behavior. This struct leaves
/// [`Future`]s returned by I/O methods pending until the I/O operation is ready
/// to avoid blocking the entire process. This allows you to start multiple I/O
/// tasks and wait for them to complete concurrently on a single thread. This
/// struct also provides methods for waiting for signals and waiting for a
/// specified duration, which are represented as [`Future`]s as well. The
/// `select` method of this struct consolidates blocking behavior into a single
/// system call so that the process can resume execution as soon as any of the
/// specified events occurs.
///
/// For system calls that do not block, such as [`Pipe`], the wrapper directly
/// forwards the call to the inner system without any modification.
///
/// [`Pipe`]: super::Pipe
#[derive(Clone, Debug)]
pub struct Concurrent<S> {
    inner: S,
}

impl<S> Concurrent<S> {
    /// Creates a new `Concurrent` system that wraps the given inner system.
    #[must_use]
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Read for Rc<Concurrent<S>>
where
    S: Read,
{
    fn read<'a>(
        &self,
        fd: Fd,
        buffer: &'a mut [u8],
    ) -> impl Future<Output = Result<usize>> + use<'a, S> {
        async move { todo!("read({:?}, {:?})", fd, buffer) }
    }
}

impl<S> Write for Rc<Concurrent<S>>
where
    S: Write,
{
    fn write<'a>(
        &self,
        fd: Fd,
        buffer: &'a [u8],
    ) -> impl Future<Output = Result<usize>> + use<'a, S> {
        async move { todo!("write({:?}, {:?})", fd, buffer) }
    }
}

impl<S> Select for Rc<Concurrent<S>>
where
    S: Select,
{
    fn select<'a>(
        &self,
        readers: &'a mut Vec<Fd>,
        writers: &'a mut Vec<Fd>,
        timeout: Option<std::time::Duration>,
        signal_mask: Option<&[signal::Number]>,
    ) -> impl Future<Output = Result<c_int>> + use<'a, S> {
        let signal_mask = signal_mask.map(|mask| mask.to_owned());
        async move {
            todo!(
                "select({:?}, {:?}, {:?}, {:?})",
                readers,
                writers,
                timeout,
                signal_mask
            )
        }
    }
}

mod delegates;
