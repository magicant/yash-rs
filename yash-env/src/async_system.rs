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

//! `AsyncSystem` implementation.

use crate::io::Fd;
use crate::ChildProcess;
use crate::System;
use futures::future::poll_fn;
use futures::task::Poll;
use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::sys::select::FdSet;
use nix::sys::wait::WaitStatus;
use std::cell::RefCell;
use std::convert::Infallible;
use std::ffi::CStr;
use std::ffi::CString;
use std::future::Future;
use std::ops::Deref;
use std::ops::DerefMut;
use std::os::raw::c_int;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Waker;

/// [System] shared by a reference counter.
///
/// TODO Elaborate
#[derive(Clone, Debug)]
pub struct SharedSystem(pub Rc<RefCell<AsyncSystem>>);

impl SharedSystem {
    /// Creates a new shared system.
    pub fn new(system: Box<dyn System>) -> Self {
        SharedSystem(Rc::new(RefCell::new(AsyncSystem::new(system))))
    }

    /// Clones this `SharedSystem` using the provided `System`.
    ///
    /// This function clones the internal state of the `SharedSystem`. However,
    /// `System` does not implement `Clone` and an instance for the cloned
    /// `SharedSystem` must be provided.
    pub fn clone_with_system(&self, system: Box<dyn System>) -> Self {
        SharedSystem(Rc::new(RefCell::new(
            self.0.borrow().clone_with_system(system),
        )))
    }

    /// Reads from the file descriptor.
    ///
    /// This function waits for one or more bytes to be available for reading.
    /// If successful, returns the number of bytes read.
    pub async fn read_async(&mut self, fd: Fd, buffer: &mut [u8]) -> nix::Result<usize> {
        let mut inner = self.0.borrow_mut();
        let flags = inner.system.fcntl_getfl(fd)?;
        if !flags.contains(OFlag::O_NONBLOCK) {
            inner.system.fcntl_setfl(fd, flags | OFlag::O_NONBLOCK)?;
        }
        drop(inner);

        poll_fn(|context| {
            let mut inner = self.0.borrow_mut();
            match inner.system.read(fd, buffer) {
                Err(Errno::EAGAIN) => {
                    inner.io.wait_for_reading(fd, context.waker().clone());
                    Poll::Pending
                }
                result => {
                    if !flags.contains(OFlag::O_NONBLOCK) {
                        let _ = inner.system.fcntl_setfl(fd, flags);
                    }
                    Poll::Ready(result)
                }
            }
        })
        .await
    }

    /// Writes to the file descriptor.
    ///
    /// This function calls [`write`](System::write) repeatedly until the whole
    /// `buffer` is written to the FD. If the `buffer` is empty, `write` is not
    /// called at all, so any error that would be returned from `write` is not
    /// returned.
    pub async fn write_all(&mut self, fd: Fd, buffer: &[u8]) -> nix::Result<usize> {
        // TODO Retry if the entire buffer could not be written in one time.
        self.0.borrow_mut().write(fd, buffer)
    }

    /// Wait for a next event to occur.
    ///
    /// TODO Elaborate
    ///
    /// This function may wake up awaiters spuriously. That is, an awaiter may
    /// be woken even if an event it is waiting for has not occurred.
    pub fn select(&self) -> nix::Result<()> {
        let mut inner = self.0.borrow_mut();
        let mut readers = inner.io.readers();
        let mut writers = inner.io.writers();
        match inner.system.select(&mut readers, &mut writers) {
            Ok(_) => {
                inner.io.wake(readers, writers);
                Ok(())
            }
            Err(Errno::EBADF) => {
                // Some of the readers and writers are invalid but we cannot
                // tell which, so we wake up everything.
                inner.io.wake_all();
                Err(Errno::EBADF)
            }
            Err(error) => Err(error),
        }
        // TODO Support timers
        // TODO Support signal catchers
    }
}

impl Deref for SharedSystem {
    type Target = Rc<RefCell<AsyncSystem>>;
    fn deref(&self) -> &Rc<RefCell<AsyncSystem>> {
        &self.0
    }
}

impl DerefMut for SharedSystem {
    fn deref_mut(&mut self) -> &mut Rc<RefCell<AsyncSystem>> {
        &mut self.0
    }
}

impl System for SharedSystem {
    fn is_executable_file(&self, path: &CStr) -> bool {
        self.borrow().is_executable_file(path)
    }
    fn pipe(&mut self) -> nix::Result<(Fd, Fd)> {
        self.borrow_mut().pipe()
    }
    fn dup(&mut self, from: Fd, to_min: Fd, cloexec: bool) -> nix::Result<Fd> {
        self.borrow_mut().dup(from, to_min, cloexec)
    }
    fn dup2(&mut self, from: Fd, to: Fd) -> nix::Result<Fd> {
        self.borrow_mut().dup2(from, to)
    }
    fn close(&mut self, fd: Fd) -> nix::Result<()> {
        self.borrow_mut().close(fd)
    }
    fn fcntl_getfl(&self, fd: Fd) -> nix::Result<OFlag> {
        self.borrow().fcntl_getfl(fd)
    }
    fn fcntl_setfl(&mut self, fd: Fd, flags: OFlag) -> nix::Result<()> {
        self.borrow_mut().fcntl_setfl(fd, flags)
    }
    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> nix::Result<usize> {
        self.borrow_mut().read(fd, buffer)
    }
    fn write(&mut self, fd: Fd, buffer: &[u8]) -> nix::Result<usize> {
        self.borrow_mut().write(fd, buffer)
    }
    fn select(&mut self, readers: &mut FdSet, writers: &mut FdSet) -> nix::Result<c_int> {
        self.borrow_mut().select(readers, writers)
    }
    unsafe fn new_child_process(&mut self) -> nix::Result<Box<dyn ChildProcess>> {
        self.borrow_mut().new_child_process()
    }
    fn wait(&mut self) -> nix::Result<WaitStatus> {
        self.borrow_mut().wait()
    }
    fn wait_sync(&mut self) -> Pin<Box<dyn Future<Output = nix::Result<WaitStatus>> + '_>> {
        panic!("SharedSystem does not support wait_sync")
    }
    fn execve(
        &mut self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> nix::Result<Infallible> {
        self.borrow_mut().execve(path, args, envs)
    }
}

/// [System] extended with asynchronous functions.
///
/// An `AsyncSystem` wraps a `System` to provide an asynchronous version of I/O,
/// signal handling, and timer functions. The wrapped `System` can be accessed
/// via the `Deref` and `DerefMut` implementations.
///
/// TODO Elaborate
#[derive(Debug)]
pub struct AsyncSystem {
    system: Box<dyn System>,
    io: AsyncIo,
}

impl Deref for AsyncSystem {
    type Target = Box<dyn System>;
    fn deref(&self) -> &Box<dyn System> {
        &self.system
    }
}

impl DerefMut for AsyncSystem {
    fn deref_mut(&mut self) -> &mut Box<dyn System> {
        &mut self.system
    }
}

impl AsyncSystem {
    /// Creates a new `AsyncSystem` that wraps the given `System`.
    pub fn new(system: Box<dyn System>) -> Self {
        AsyncSystem {
            system,
            io: AsyncIo::new(),
        }
    }

    /// Clones this `AsyncSystem` using the provided `System`.
    ///
    /// This function clones the internal state of the `AsyncSystem`. However,
    /// `System` does not implement `Clone` and an instance for the cloned
    /// `AsyncSystem` must be provided.
    pub fn clone_with_system(&self, system: Box<dyn System>) -> Self {
        AsyncSystem {
            system,
            io: self.io.clone(),
        }
    }
}

/// Helper for `select`ing on FDs.
///
/// An `AsyncIo` is a set of [Waker]s that are waiting for an FD to be ready for
/// reading or writing.
///
/// TODO Elaborate
#[derive(Clone, Debug, Default)]
struct AsyncIo {
    readers: Vec<Awaiter>,
    writers: Vec<Awaiter>,
}

#[derive(Clone, Debug)]
struct Awaiter {
    fd: Fd,
    waker: Waker,
}

impl AsyncIo {
    /// Returns a new empty `AsyncIo`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a set of FDs waiting for reading.
    ///
    /// The return value should be passed to the `select` or `pselect` system
    /// call.
    pub fn readers(&self) -> FdSet {
        let mut set = FdSet::new();
        for reader in &self.readers {
            set.insert(reader.fd.0);
        }
        set
    }

    /// Returns a set of FDs waiting for writing.
    ///
    /// The return value should be passed to the `select` or `pselect` system
    /// call.
    pub fn writers(&self) -> FdSet {
        let mut set = FdSet::new();
        for writer in &self.writers {
            set.insert(writer.fd.0);
        }
        set
    }

    /// Adds an awaiter for reading.
    pub fn wait_for_reading(&mut self, fd: Fd, waker: Waker) {
        self.readers.push(Awaiter { fd, waker });
    }

    /// Adds an awaiter for writing.
    pub fn wait_for_writing(&mut self, fd: Fd, waker: Waker) {
        self.writers.push(Awaiter { fd, waker });
    }

    /// Wakes awaiters that are ready for reading/writing.
    ///
    /// FDs in `readers` and `writers` are considered ready and corresponding
    /// awaiters are woken. Once woken, awaiters are removed from `self`.
    pub fn wake(&mut self, mut readers: FdSet, mut writers: FdSet) {
        for i in (0..self.readers.len()).rev() {
            if readers.contains(self.readers[i].fd.0) {
                self.readers.remove(i).waker.wake();
            }
        }
        for i in (0..self.writers.len()).rev() {
            if writers.contains(self.writers[i].fd.0) {
                self.writers.remove(i).waker.wake();
            }
        }
    }

    /// Wakes and removes all awaiters.
    pub fn wake_all(&mut self) {
        self.readers.drain(..).for_each(|a| a.waker.wake());
        self.writers.drain(..).for_each(|a| a.waker.wake());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virtual_system::VirtualSystem;
    use futures::executor::block_on;
    use futures::task::noop_waker;
    use futures::task::noop_waker_ref;
    use std::future::Future;
    use std::rc::Rc;
    use std::task::Context;

    #[test]
    fn shared_system_read_async_ready() {
        let mut system = SharedSystem::new(Box::new(VirtualSystem::new()));
        let (reader, writer) = system.pipe().unwrap();
        system.write(writer, &[42]).unwrap();

        let mut buffer = [0; 2];
        let result = block_on(system.read_async(reader, &mut buffer));
        assert_eq!(result, Ok(1));
        assert_eq!(buffer[..1], [42]);
    }

    #[test]
    fn shared_system_read_async_not_ready_at_first() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        let system2 = system.clone();
        let (reader, writer) = system.pipe().unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut buffer = [0; 2];
        let mut future = Box::pin(system.read_async(reader, &mut buffer));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        let result = system2.select();
        assert_eq!(result, Ok(()));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        let state = state.borrow_mut();
        let mut ofd = state.processes[&process_id].fds[&writer]
            .open_file_description
            .borrow_mut();
        ofd.write(&[56]).unwrap();
        drop(ofd);
        drop(state);

        let result = future.as_mut().poll(&mut context);
        drop(future);
        assert_eq!(result, Poll::Ready(Ok(1)));
        assert_eq!(buffer[..1], [56]);
    }

    #[test]
    fn shared_system_write_all_ready() {
        let mut system = SharedSystem::new(Box::new(VirtualSystem::new()));
        let (reader, writer) = system.pipe().unwrap();
        let result = block_on(system.write_all(writer, &[17]));
        assert_eq!(result, Ok(1));

        let mut buffer = [0; 2];
        system.read(reader, &mut buffer).unwrap();
        assert_eq!(buffer[..1], [17]);
    }

    // TODO test shared_system_write_all_not_ready_at_first

    #[test]
    fn async_io_has_no_default_readers_or_writers() {
        let async_io = AsyncIo::new();
        assert_eq!(async_io.readers(), FdSet::new());
        assert_eq!(async_io.writers(), FdSet::new());
    }

    #[test]
    fn async_io_non_empty_readers_and_writers() {
        let mut async_io = AsyncIo::new();
        async_io.wait_for_reading(Fd::STDIN, noop_waker());
        async_io.wait_for_writing(Fd::STDOUT, noop_waker());
        async_io.wait_for_writing(Fd::STDERR, noop_waker());

        let mut expected_readers = FdSet::new();
        expected_readers.insert(Fd::STDIN.0);
        let mut expected_writers = FdSet::new();
        expected_writers.insert(Fd::STDOUT.0);
        expected_writers.insert(Fd::STDERR.0);
        assert_eq!(async_io.readers(), expected_readers);
        assert_eq!(async_io.writers(), expected_writers);
    }

    #[test]
    fn async_io_wake() {
        let mut async_io = AsyncIo::new();
        async_io.wait_for_reading(Fd(3), noop_waker());
        async_io.wait_for_reading(Fd(4), noop_waker());
        async_io.wait_for_writing(Fd(4), noop_waker());
        let mut fds = FdSet::new();
        fds.insert(4);
        async_io.wake(fds, fds);

        let mut expected_readers = FdSet::new();
        expected_readers.insert(3);
        assert_eq!(async_io.readers(), expected_readers);
        assert_eq!(async_io.writers(), FdSet::new());
    }

    #[test]
    fn async_io_wake_all() {
        let mut async_io = AsyncIo::new();
        async_io.wait_for_reading(Fd::STDIN, noop_waker());
        async_io.wait_for_writing(Fd::STDOUT, noop_waker());
        async_io.wait_for_writing(Fd::STDERR, noop_waker());
        async_io.wake_all();
        assert_eq!(async_io.readers(), FdSet::new());
        assert_eq!(async_io.writers(), FdSet::new());
    }
}
