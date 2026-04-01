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

use super::{Pipe, Read, Select, Write};

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
