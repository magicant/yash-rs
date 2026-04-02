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

use super::{
    Close, Dir, Dup, Fcntl, FdFlag, Fstat, IsExecutableFile, OfdAccess, Open, Pipe, Read, Result,
    Select, Write,
};
use crate::io::Fd;
use enumset::EnumSet;
use std::ffi::CStr;
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

impl<S> Fstat for Rc<Concurrent<S>>
where
    S: Fstat,
{
    type Stat = S::Stat;

    #[inline]
    fn fstat(&self, fd: Fd) -> Result<Self::Stat> {
        self.inner.fstat(fd)
    }

    #[inline]
    fn fstatat(&self, dir_fd: Fd, path: &CStr, follow_symlinks: bool) -> Result<Self::Stat> {
        self.inner.fstatat(dir_fd, path, follow_symlinks)
    }

    #[inline]
    fn is_directory(&self, path: &CStr) -> bool {
        self.inner.is_directory(path)
    }

    #[inline]
    fn fd_is_pipe(&self, fd: Fd) -> bool {
        self.inner.fd_is_pipe(fd)
    }
}

impl<S> IsExecutableFile for Rc<Concurrent<S>>
where
    S: IsExecutableFile,
{
    #[inline]
    fn is_executable_file(&self, path: &CStr) -> bool {
        self.inner.is_executable_file(path)
    }
}

impl<S> Pipe for Rc<Concurrent<S>>
where
    S: Pipe,
{
    #[inline]
    fn pipe(&self) -> Result<(Fd, Fd)> {
        self.inner.pipe()
    }
}

impl<S> Dup for Rc<Concurrent<S>>
where
    S: Dup,
{
    #[inline]
    fn dup(&self, from: Fd, to_min: Fd, flags: EnumSet<FdFlag>) -> Result<Fd> {
        self.inner.dup(from, to_min, flags)
    }

    #[inline]
    fn dup2(&self, from: Fd, to: Fd) -> Result<Fd> {
        self.inner.dup2(from, to)
    }
}

/// This implementation does not (yet) support non-blocking open operations.
impl<S> Open for Rc<Concurrent<S>>
where
    S: Open,
{
    #[inline]
    fn open(
        &self,
        path: &CStr,
        access: OfdAccess,
        flags: EnumSet<super::OpenFlag>,
        mode: super::Mode,
    ) -> impl Future<Output = Result<Fd>> + use<S> {
        self.inner.open(path, access, flags, mode)
    }

    #[inline]
    fn open_tmpfile(&self, parent_dir: &unix_path::Path) -> Result<Fd> {
        self.inner.open_tmpfile(parent_dir)
    }

    #[inline]
    fn fdopendir(&self, fd: Fd) -> Result<impl Dir + use<S>> {
        self.inner.fdopendir(fd)
    }

    #[inline]
    fn opendir(&self, path: &CStr) -> Result<impl Dir + use<S>> {
        self.inner.opendir(path)
    }
}

impl<S> Close for Rc<Concurrent<S>>
where
    S: Close,
{
    #[inline]
    fn close(&self, fd: Fd) -> Result<()> {
        self.inner.close(fd)
    }
}

impl<S> Fcntl for Rc<Concurrent<S>>
where
    S: Fcntl,
{
    #[inline]
    fn ofd_access(&self, fd: Fd) -> Result<OfdAccess> {
        self.inner.ofd_access(fd)
    }

    #[inline]
    fn get_and_set_nonblocking(&self, fd: Fd, nonblocking: bool) -> Result<bool> {
        self.inner.get_and_set_nonblocking(fd, nonblocking)
    }

    #[inline]
    fn fcntl_getfd(&self, fd: Fd) -> Result<EnumSet<FdFlag>> {
        self.inner.fcntl_getfd(fd)
    }

    #[inline]
    fn fcntl_setfd(&self, fd: Fd, flags: EnumSet<FdFlag>) -> Result<()> {
        self.inner.fcntl_setfd(fd, flags)
    }
}
