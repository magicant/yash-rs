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

//! Extension of `Concurrent` for repeating reads and writes until all data is processed

use super::{Concurrent, TemporaryNonBlockingGuard};
use crate::io::Fd;
use crate::system::{Errno, Fcntl, Read, Write};
use std::cell::{Cell, LazyCell};
use std::iter::repeat_n;
use std::rc::Rc;

impl<S> Concurrent<S>
where
    S: Fcntl + Read,
{
    /// Reads from the file descriptor until EOF is reached, appending the data
    /// to the provided buffer.
    ///
    /// In case of an error, the buffer will contain all data read up to the
    /// point of failure.
    ///
    /// Use [`read_all`](Self::read_all) if you don't have an existing buffer to
    /// append to.
    pub async fn read_all_to(&self, fd: Fd, buffer: &mut Vec<u8>) -> Result<(), Errno> {
        let this = TemporaryNonBlockingGuard::new(self, fd);
        let waker = LazyCell::new(|| Rc::new(Cell::new(None)));
        let mut effective_length = buffer.len();
        loop {
            // The `read` method requires an initialized buffer, so we reserve
            // additional capacity and fill it with zeros.
            let unused = buffer.capacity() - effective_length;
            buffer.reserve(0x400_usize.saturating_sub(unused));
            buffer.extend(repeat_n(0, buffer.capacity() - buffer.len()));

            match this.inner.read(fd, &mut buffer[effective_length..]).await {
                Ok(0) => {
                    buffer.truncate(effective_length);
                    return Ok(());
                }
                Ok(n) => {
                    effective_length += n;
                }

                // EWOULDBLOCK is unreachable if it has the same value as EAGAIN.
                #[allow(unreachable_patterns)]
                Err(Errno::EAGAIN | Errno::EWOULDBLOCK) => this.yield_for_read(fd, &waker).await,

                Err(e) => {
                    buffer.truncate(effective_length);
                    return Err(e);
                }
            }
        }
    }

    /// Reads from the file descriptor until EOF is reached, returning the
    /// collected data as a `Vec<u8>`.
    ///
    /// This is a convenience method that allocates a buffer and calls
    /// [`read_all_to`](Self::read_all_to).
    pub async fn read_all(&self, fd: Fd) -> Result<Vec<u8>, Errno> {
        let mut buffer = Vec::new();
        self.read_all_to(fd, &mut buffer).await?;
        Ok(buffer)
    }
}

impl<S> Concurrent<S>
where
    S: Fcntl + Write,
{
    /// Writes all data from the provided buffer to the file descriptor.
    ///
    /// This method ensures that all data is written, even if multiple write
    /// operations are required due to partial writes.
    ///
    /// If the data is empty, this method will return immediately without
    /// performing write operations.
    pub async fn write_all(&self, fd: Fd, mut data: &[u8]) -> Result<(), Errno> {
        if data.is_empty() {
            return Ok(());
        }

        let this = TemporaryNonBlockingGuard::new(self, fd);
        let waker = LazyCell::new(|| Rc::new(Cell::new(None)));
        loop {
            match this.inner.write(fd, data).await {
                // EWOULDBLOCK is unreachable if it has the same value as EAGAIN.
                #[allow(unreachable_patterns)]
                Ok(0) | Err(Errno::EAGAIN | Errno::EWOULDBLOCK) => {
                    this.yield_for_write(fd, &waker).await
                }

                Ok(n) => {
                    data = &data[n..];
                    if data.is_empty() {
                        return Ok(());
                    }
                }

                Err(e) => return Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::r#virtual::{PIPE_SIZE, VirtualSystem};
    use crate::system::{Close as _, Mode, OfdAccess, Open as _, OpenFlag, Pipe as _};
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_executor::Executor;
    use yash_executor::forwarder::TryReceiveError;

    #[test]
    fn read_all_and_write_all_transfer_all_data() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let (read_fd, write_fd) = system.pipe().unwrap();

        // Prepare a large buffer of data to write
        let mut source = [0; PIPE_SIZE * 10];
        for (i, byte) in source.iter_mut().enumerate() {
            *byte = ((i * 37 + 13) % 256) as u8;
        }

        let executor = Executor::new();
        let read = unsafe { executor.spawn(system.read_all(read_fd)) };
        let write = unsafe {
            executor.spawn(async {
                let result = system.write_all(write_fd, &source).await;
                assert_eq!(result, Ok(()));
                let result = system.close(write_fd);
                assert_eq!(result, Ok(()));
            })
        };

        // Run both operations concurrently
        let transferred = loop {
            executor.run_until_stalled();

            match read.try_receive() {
                Ok(result) => break result,
                Err(TryReceiveError::NotSent) => {
                    // The read operation is not complete yet, so we continue running the executor
                }
                Err(e) => panic!("unexpected error: {e:?}"),
            }

            system.select().now_or_never().unwrap();
        };

        assert_eq!(transferred.unwrap(), source);
        assert_eq!(write.try_receive(), Ok(()));
    }

    #[test]
    fn read_all_preserves_fd_blocking_mode() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let fd = system
            .open(
                c"/foo",
                OfdAccess::ReadOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap()
            .unwrap();

        system.read_all(fd).now_or_never().unwrap().unwrap();

        // The file descriptor should have the same blocking mode as before
        // (which is blocking by default)
        assert_eq!(system.inner.get_and_set_nonblocking(fd, false), Ok(false));

        system.inner.get_and_set_nonblocking(fd, true).ok();
        system.read_all(fd).now_or_never().unwrap().unwrap();
        // The file descriptor should have the same blocking mode as before
        // (which was set to non-blocking before the read)
        assert_eq!(system.inner.get_and_set_nonblocking(fd, true), Ok(true));
    }

    #[test]
    fn write_all_preserves_fd_blocking_mode() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let fd = system
            .open(
                c"/foo",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap()
            .unwrap();

        system
            .write_all(fd, b"hello")
            .now_or_never()
            .unwrap()
            .unwrap();

        // The file descriptor should have the same blocking mode as before
        // (which is blocking by default)
        assert_eq!(system.inner.get_and_set_nonblocking(fd, false), Ok(false));

        system.inner.get_and_set_nonblocking(fd, true).ok();
        system
            .write_all(fd, b"world")
            .now_or_never()
            .unwrap()
            .unwrap();
        // The file descriptor should have the same blocking mode as before
        // (which was set to non-blocking before the write)
        assert_eq!(system.inner.get_and_set_nonblocking(fd, true), Ok(true));
    }
}
