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

//! [`SharedSystem`] and related items

use super::signal;
use super::ChildProcessStarter;
use super::Dir;
use super::Errno;
use super::FdSet;
use super::Gid;
use super::LimitPair;
use super::Resource;
use super::Result;
use super::SelectSystem;
use super::SignalHandling;
use super::SignalStatus;
use super::SignalSystem;
use super::System;
use super::SystemEx;
use super::Times;
use super::Uid;
use crate::io::Fd;
use crate::job::Pid;
use crate::job::ProcessState;
#[cfg(doc)]
use crate::Env;
use nix::fcntl::AtFlags;
use nix::fcntl::FdFlag;
use nix::fcntl::OFlag;
use nix::sys::signal::SigmaskHow;
use nix::sys::stat::{FileStat, Mode};
use nix::sys::time::TimeSpec;
use std::cell::RefCell;
use std::convert::Infallible;
use std::ffi::c_int;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsString;
use std::future::poll_fn;
use std::future::Future;
use std::io::SeekFrom;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Poll;
use std::time::Instant;

/// System shared by a reference counter.
///
/// A `SharedSystem` is a reference-counted container of a [`System`] instance
/// accompanied with an internal state for supporting asynchronous interactions
/// with the system. As it is reference-counted, cloning a `SharedSystem`
/// instance only increments the reference count without cloning the backing
/// system instance. This behavior allows calling `SharedSystem`'s methods
/// concurrently from different `async` tasks that each have a `SharedSystem`
/// instance sharing the same state.
///
/// `SharedSystem` implements [`System`] by delegating to the contained system
/// instance. You should avoid calling some of the `System` methods, however.
/// Prefer `async` functions provided by `SharedSystem` (e.g.,
/// [`read_async`](Self::read_async)) over raw system functions (e.g.,
/// [`read`](System::read)).
///
/// The following example illustrates how multiple concurrent tasks are run in a
/// single-threaded pool:
///
/// ```
/// # use yash_env::{SharedSystem, System, VirtualSystem};
/// # use futures_util::task::LocalSpawnExt;
/// let mut system = SharedSystem::new(Box::new(VirtualSystem::new()));
/// let mut system2 = system.clone();
/// let mut system3 = system.clone();
/// let (reader, writer) = system.pipe().unwrap();
/// let mut executor = futures_executor::LocalPool::new();
///
/// // We add a task that tries to read from the pipe, but nothing has been
/// // written to it, so the task is stalled.
/// let read_task = executor.spawner().spawn_local_with_handle(async move {
///     let mut buffer = [0; 1];
///     system.read_async(reader, &mut buffer).await.unwrap();
///     buffer[0]
/// });
/// executor.run_until_stalled();
///
/// // Let's add a task that writes to the pipe.
/// executor.spawner().spawn_local(async move {
///     system2.write_all(writer, &[123]).await.unwrap();
/// });
/// executor.run_until_stalled();
///
/// // The write task has written a byte to the pipe, but the read task is still
/// // stalled. We need to wake it up by calling `select`.
/// system3.select(false).unwrap();
///
/// // Now the read task can proceed to the end.
/// let number = executor.run_until(read_task.unwrap());
/// assert_eq!(number, 123);
/// ```
///
/// If there is a child process in the [`VirtualSystem`], you should call
/// [`SystemState::select_all`](super::virtual::SystemState::select_all) in
/// addition to [`SharedSystem::select`] so that the child process task is woken
/// up when needed.
/// (TBD code example)
///
/// [`VirtualSystem`]: crate::system::virtual::VirtualSystem
#[derive(Clone, Debug)]
pub struct SharedSystem(pub(super) Rc<RefCell<SelectSystem>>);

impl SharedSystem {
    /// Creates a new shared system.
    pub fn new(system: Box<dyn System>) -> Self {
        SharedSystem(Rc::new(RefCell::new(SelectSystem::new(system))))
    }

    fn set_nonblocking(&self, fd: Fd) -> Result<OFlag> {
        let mut inner = self.0.borrow_mut();
        let flags = inner.fcntl_getfl(fd)?;
        if !flags.contains(OFlag::O_NONBLOCK) {
            inner.fcntl_setfl(fd, flags | OFlag::O_NONBLOCK)?;
        }
        Ok(flags)
    }

    fn reset_nonblocking(&self, fd: Fd, old_flags: OFlag) {
        if !old_flags.contains(OFlag::O_NONBLOCK) {
            let _: Result<()> = self.0.borrow_mut().fcntl_setfl(fd, old_flags);
        }
    }

    /// Reads from the file descriptor.
    ///
    /// This function waits for one or more bytes to be available for reading.
    /// If successful, returns the number of bytes read.
    pub async fn read_async(&self, fd: Fd, buffer: &mut [u8]) -> Result<usize> {
        let flags = self.set_nonblocking(fd)?;

        // We need to retain a strong reference to the waker outside the poll_fn
        // function because SelectSystem only retains a weak reference to it.
        // This allows SelectSystem to discard defunct wakers if this async task
        // is aborted.
        let waker = Rc::new(RefCell::new(None));

        let result = poll_fn(|context| {
            let mut inner = self.0.borrow_mut();
            match inner.read(fd, buffer) {
                Err(Errno::EAGAIN) => {
                    *waker.borrow_mut() = Some(context.waker().clone());
                    inner.add_reader(fd, Rc::downgrade(&waker));
                    Poll::Pending
                }
                result => Poll::Ready(result),
            }
        })
        .await;

        self.reset_nonblocking(fd, flags);

        result
    }

    /// Writes to the file descriptor.
    ///
    /// This function calls [`System::write`] repeatedly until the whole
    /// `buffer` is written to the FD. If the `buffer` is empty, `write` is not
    /// called at all, so any error that would be returned from `write` is not
    /// returned.
    ///
    /// This function silently ignores signals that may interrupt writes.
    pub async fn write_all(&self, fd: Fd, mut buffer: &[u8]) -> Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }

        let flags = self.set_nonblocking(fd)?;
        let mut written = 0;

        // We need to retain a strong reference to the waker outside the poll_fn
        // function because SelectSystem only retains a weak reference to it.
        // This allows SelectSystem to discard defunct wakers if this async task
        // is aborted.
        let waker = Rc::new(RefCell::new(None));

        let result = poll_fn(|context| {
            let mut inner = self.0.borrow_mut();
            match inner.write(fd, buffer) {
                Ok(count) => {
                    written += count;
                    buffer = &buffer[count..];
                    if buffer.is_empty() {
                        return Poll::Ready(Ok(written));
                    }
                }
                Err(Errno::EAGAIN | Errno::EINTR) => (),
                Err(error) => return Poll::Ready(Err(error)),
            }

            *waker.borrow_mut() = Some(context.waker().clone());
            inner.add_writer(fd, Rc::downgrade(&waker));
            Poll::Pending
        })
        .await;

        self.reset_nonblocking(fd, flags);

        result
    }

    /// Convenience function for printing a message to the standard error
    pub async fn print_error(&self, message: &str) {
        _ = self.write_all(Fd::STDERR, message.as_bytes()).await;
    }

    /// Waits until the specified time point.
    pub async fn wait_until(&self, target: Instant) {
        // We need to retain a strong reference to the waker outside the poll_fn
        // function because SelectSystem only retains a weak reference to it.
        // This allows SelectSystem to discard defunct wakers if this async task
        // is aborted.
        let waker = Rc::new(RefCell::new(None));

        poll_fn(|context| {
            let mut system = self.0.borrow_mut();
            let now = system.now();
            if now >= target {
                return Poll::Ready(());
            }
            *waker.borrow_mut() = Some(context.waker().clone());
            system.add_timeout(target, Rc::downgrade(&waker));
            Poll::Pending
        })
        .await
    }

    /// Waits for some signals to be delivered to this process.
    ///
    /// Before calling this function, you need to [set signal
    /// handling](Self::set_signal_handling) to `Catch`. Without doing so, this
    /// function cannot detect the receipt of the signals.
    ///
    /// Returns an array of signals that were caught.
    ///
    /// If this `SharedSystem` is part of an [`Env`], you should call
    /// [`Env::wait_for_signals`] rather than calling this function directly
    /// so that the trap set can remember the caught signal.
    pub async fn wait_for_signals(&self) -> Rc<[signal::Number]> {
        let status = self.0.borrow_mut().add_signal_waker();
        poll_fn(|context| {
            let mut status = status.borrow_mut();
            let dummy_status = SignalStatus::Expected(None);
            let old_status = std::mem::replace(&mut *status, dummy_status);
            match old_status {
                SignalStatus::Caught(signals) => Poll::Ready(signals),
                SignalStatus::Expected(_) => {
                    *status = SignalStatus::Expected(Some(context.waker().clone()));
                    Poll::Pending
                }
            }
        })
        .await
    }

    /// Waits for a signal to be delivered to this process.
    ///
    /// Before calling this function, you need to [set signal
    /// handling](Self::set_signal_handling) to `Catch`.
    /// Without doing so, this function cannot detect the receipt of the signal.
    ///
    /// If this `SharedSystem` is part of an [`Env`], you should call
    /// [`Env::wait_for_signal`] rather than calling this function directly
    /// so that the trap set can remember the caught signal.
    pub async fn wait_for_signal(&self, signal: signal::Number) {
        while !self.wait_for_signals().await.contains(&signal) {}
    }

    /// Waits for a next event to occur.
    ///
    /// This function calls [`System::select`] with arguments computed from the
    /// current internal state of the `SharedSystem`. It will wake up tasks
    /// waiting for the file descriptor to be ready in
    /// [`read_async`](Self::read_async) and [`write_all`](Self::write_all) or
    /// for a signal to be caught in [`wait_for_signal`](Self::wait_for_signal).
    /// If no tasks are woken for FDs or signals and `poll` is false, this
    /// function will block until the first task waiting for a specific time
    /// point is woken.
    ///
    /// If poll is true, this function does not block, so it may not wake up any
    /// tasks.
    ///
    /// This function may wake up a task even if the condition it is expecting
    /// has not yet been met.
    pub fn select(&self, poll: bool) -> Result<()> {
        self.0.borrow_mut().select(poll)
    }
}

/// Delegates `System` methods to the contained system instance.
///
/// This implementation only requires a non-mutable reference to the shared
/// system because it uses `RefCell` to access the contained system instance.
impl System for &SharedSystem {
    fn fstat(&self, fd: Fd) -> Result<FileStat> {
        self.0.borrow().fstat(fd)
    }
    fn fstatat(&self, dir_fd: Fd, path: &CStr, flags: AtFlags) -> Result<FileStat> {
        self.0.borrow().fstatat(dir_fd, path, flags)
    }
    fn is_executable_file(&self, path: &CStr) -> bool {
        self.0.borrow().is_executable_file(path)
    }
    fn is_directory(&self, path: &CStr) -> bool {
        self.0.borrow().is_directory(path)
    }
    fn pipe(&mut self) -> Result<(Fd, Fd)> {
        self.0.borrow_mut().pipe()
    }
    fn dup(&mut self, from: Fd, to_min: Fd, flags: FdFlag) -> Result<Fd> {
        self.0.borrow_mut().dup(from, to_min, flags)
    }
    fn dup2(&mut self, from: Fd, to: Fd) -> Result<Fd> {
        self.0.borrow_mut().dup2(from, to)
    }
    fn open(&mut self, path: &CStr, option: OFlag, mode: Mode) -> Result<Fd> {
        self.0.borrow_mut().open(path, option, mode)
    }
    fn open_tmpfile(&mut self, parent_dir: &Path) -> Result<Fd> {
        self.0.borrow_mut().open_tmpfile(parent_dir)
    }
    fn close(&mut self, fd: Fd) -> Result<()> {
        self.0.borrow_mut().close(fd)
    }
    fn fcntl_getfl(&self, fd: Fd) -> Result<OFlag> {
        self.0.borrow().fcntl_getfl(fd)
    }
    fn fcntl_setfl(&mut self, fd: Fd, flags: OFlag) -> Result<()> {
        self.0.borrow_mut().fcntl_setfl(fd, flags)
    }
    fn fcntl_getfd(&self, fd: Fd) -> Result<FdFlag> {
        self.0.borrow().fcntl_getfd(fd)
    }
    fn fcntl_setfd(&mut self, fd: Fd, flags: FdFlag) -> Result<()> {
        self.0.borrow_mut().fcntl_setfd(fd, flags)
    }
    fn isatty(&self, fd: Fd) -> Result<bool> {
        self.0.borrow().isatty(fd)
    }
    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> Result<usize> {
        self.0.borrow_mut().read(fd, buffer)
    }
    fn write(&mut self, fd: Fd, buffer: &[u8]) -> Result<usize> {
        self.0.borrow_mut().write(fd, buffer)
    }
    fn lseek(&mut self, fd: Fd, position: SeekFrom) -> Result<u64> {
        self.0.borrow_mut().lseek(fd, position)
    }
    fn fdopendir(&mut self, fd: Fd) -> Result<Box<dyn Dir>> {
        self.0.borrow_mut().fdopendir(fd)
    }
    fn opendir(&mut self, path: &CStr) -> Result<Box<dyn Dir>> {
        self.0.borrow_mut().opendir(path)
    }
    fn umask(&mut self, mask: Mode) -> Mode {
        self.0.borrow_mut().umask(mask)
    }
    fn now(&self) -> Instant {
        self.0.borrow().now()
    }
    fn times(&self) -> Result<Times> {
        self.0.borrow().times()
    }
    fn validate_signal(&self, number: signal::RawNumber) -> Option<(signal::Name, signal::Number)> {
        self.0.borrow().validate_signal(number)
    }
    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number> {
        self.0.borrow().signal_number_from_name(name)
    }
    fn sigmask(
        &mut self,
        op: Option<(SigmaskHow, &[signal::Number])>,
        old_mask: Option<&mut Vec<signal::Number>>,
    ) -> Result<()> {
        (**self.0.borrow_mut()).sigmask(op, old_mask)
    }
    fn sigaction(
        &mut self,
        signal: signal::Number,
        action: SignalHandling,
    ) -> Result<SignalHandling> {
        self.0.borrow_mut().sigaction(signal, action)
    }
    fn caught_signals(&mut self) -> Vec<signal::Number> {
        self.0.borrow_mut().caught_signals()
    }
    fn kill(
        &mut self,
        target: Pid,
        signal: Option<signal::Number>,
    ) -> Pin<Box<(dyn Future<Output = Result<()>>)>> {
        self.0.borrow_mut().kill(target, signal)
    }
    fn select(
        &mut self,
        readers: &mut FdSet,
        writers: &mut FdSet,
        timeout: Option<&TimeSpec>,
        signal_mask: Option<&[signal::Number]>,
    ) -> Result<c_int> {
        (**self.0.borrow_mut()).select(readers, writers, timeout, signal_mask)
    }
    fn getpid(&self) -> Pid {
        self.0.borrow().getpid()
    }
    fn getppid(&self) -> Pid {
        self.0.borrow().getppid()
    }
    fn getpgrp(&self) -> Pid {
        self.0.borrow().getpgrp()
    }
    fn setpgid(&mut self, pid: Pid, pgid: Pid) -> Result<()> {
        self.0.borrow_mut().setpgid(pid, pgid)
    }
    fn tcgetpgrp(&self, fd: Fd) -> Result<Pid> {
        self.0.borrow().tcgetpgrp(fd)
    }
    fn tcsetpgrp(&mut self, fd: Fd, pgid: Pid) -> Result<()> {
        self.0.borrow_mut().tcsetpgrp(fd, pgid)
    }
    fn new_child_process(&mut self) -> Result<ChildProcessStarter> {
        self.0.borrow_mut().new_child_process()
    }
    fn wait(&mut self, target: Pid) -> Result<Option<(Pid, ProcessState)>> {
        self.0.borrow_mut().wait(target)
    }
    fn execve(&mut self, path: &CStr, args: &[CString], envs: &[CString]) -> Result<Infallible> {
        self.0.borrow_mut().execve(path, args, envs)
    }
    fn getcwd(&self) -> Result<PathBuf> {
        self.0.borrow().getcwd()
    }
    fn chdir(&mut self, path: &CStr) -> Result<()> {
        self.0.borrow_mut().chdir(path)
    }
    fn getuid(&self) -> Uid {
        self.0.borrow().getuid()
    }
    fn geteuid(&self) -> Uid {
        self.0.borrow().geteuid()
    }
    fn getgid(&self) -> Gid {
        self.0.borrow().getgid()
    }
    fn getegid(&self) -> Gid {
        self.0.borrow().getegid()
    }
    fn getpwnam_dir(&self, name: &str) -> Result<Option<PathBuf>> {
        self.0.borrow().getpwnam_dir(name)
    }
    fn confstr_path(&self) -> Result<OsString> {
        self.0.borrow().confstr_path()
    }
    fn shell_path(&self) -> CString {
        self.0.borrow().shell_path()
    }
    fn getrlimit(&self, resource: Resource) -> std::io::Result<LimitPair> {
        self.0.borrow().getrlimit(resource)
    }
    fn setrlimit(&mut self, resource: Resource, limits: LimitPair) -> std::io::Result<()> {
        self.0.borrow_mut().setrlimit(resource, limits)
    }
}

/// Delegates `System` methods to the contained system instance.
impl System for SharedSystem {
    // All methods are delegated to `impl System for &SharedSystem`,
    // which in turn delegates to the contained system instance.
    #[inline]
    fn fstat(&self, fd: Fd) -> Result<FileStat> {
        (&self).fstat(fd)
    }
    #[inline]
    fn fstatat(&self, dir_fd: Fd, path: &CStr, flags: AtFlags) -> Result<FileStat> {
        (&self).fstatat(dir_fd, path, flags)
    }
    #[inline]
    fn is_executable_file(&self, path: &CStr) -> bool {
        (&self).is_executable_file(path)
    }
    #[inline]
    fn is_directory(&self, path: &CStr) -> bool {
        (&self).is_directory(path)
    }
    #[inline]
    fn pipe(&mut self) -> Result<(Fd, Fd)> {
        (&mut &*self).pipe()
    }
    #[inline]
    fn dup(&mut self, from: Fd, to_min: Fd, flags: FdFlag) -> Result<Fd> {
        (&mut &*self).dup(from, to_min, flags)
    }
    #[inline]
    fn dup2(&mut self, from: Fd, to: Fd) -> Result<Fd> {
        (&mut &*self).dup2(from, to)
    }
    #[inline]
    fn open(&mut self, path: &CStr, option: OFlag, mode: Mode) -> Result<Fd> {
        (&mut &*self).open(path, option, mode)
    }
    #[inline]
    fn open_tmpfile(&mut self, parent_dir: &Path) -> Result<Fd> {
        (&mut &*self).open_tmpfile(parent_dir)
    }
    #[inline]
    fn close(&mut self, fd: Fd) -> Result<()> {
        (&mut &*self).close(fd)
    }
    #[inline]
    fn fcntl_getfl(&self, fd: Fd) -> Result<OFlag> {
        (&self).fcntl_getfl(fd)
    }
    #[inline]
    fn fcntl_setfl(&mut self, fd: Fd, flags: OFlag) -> Result<()> {
        (&mut &*self).fcntl_setfl(fd, flags)
    }
    #[inline]
    fn fcntl_getfd(&self, fd: Fd) -> Result<FdFlag> {
        (&self).fcntl_getfd(fd)
    }
    #[inline]
    fn fcntl_setfd(&mut self, fd: Fd, flags: FdFlag) -> Result<()> {
        (&mut &*self).fcntl_setfd(fd, flags)
    }
    #[inline]
    fn isatty(&self, fd: Fd) -> Result<bool> {
        (&self).isatty(fd)
    }
    #[inline]
    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> Result<usize> {
        (&mut &*self).read(fd, buffer)
    }
    #[inline]
    fn write(&mut self, fd: Fd, buffer: &[u8]) -> Result<usize> {
        (&mut &*self).write(fd, buffer)
    }
    #[inline]
    fn lseek(&mut self, fd: Fd, position: SeekFrom) -> Result<u64> {
        (&mut &*self).lseek(fd, position)
    }
    #[inline]
    fn fdopendir(&mut self, fd: Fd) -> Result<Box<dyn Dir>> {
        (&mut &*self).fdopendir(fd)
    }
    #[inline]
    fn opendir(&mut self, path: &CStr) -> Result<Box<dyn Dir>> {
        (&mut &*self).opendir(path)
    }
    #[inline]
    fn umask(&mut self, mask: Mode) -> Mode {
        (&mut &*self).umask(mask)
    }
    #[inline]
    fn now(&self) -> Instant {
        (&self).now()
    }
    #[inline]
    fn times(&self) -> Result<Times> {
        (&self).times()
    }
    #[inline]
    fn validate_signal(&self, number: signal::RawNumber) -> Option<(signal::Name, signal::Number)> {
        (&self).validate_signal(number)
    }
    #[inline]
    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number> {
        System::signal_number_from_name(&self, name)
    }
    #[inline]
    fn sigmask(
        &mut self,
        op: Option<(SigmaskHow, &[signal::Number])>,
        old_mask: Option<&mut Vec<signal::Number>>,
    ) -> Result<()> {
        (&mut &*self).sigmask(op, old_mask)
    }
    #[inline]
    fn sigaction(
        &mut self,
        signal: signal::Number,
        action: SignalHandling,
    ) -> Result<SignalHandling> {
        (&mut &*self).sigaction(signal, action)
    }
    #[inline]
    fn caught_signals(&mut self) -> Vec<signal::Number> {
        (&mut &*self).caught_signals()
    }
    #[inline]
    fn kill(
        &mut self,
        target: Pid,
        signal: Option<signal::Number>,
    ) -> Pin<Box<dyn Future<Output = Result<()>>>> {
        (&mut &*self).kill(target, signal)
    }
    #[inline]
    fn select(
        &mut self,
        readers: &mut FdSet,
        writers: &mut FdSet,
        timeout: Option<&TimeSpec>,
        signal_mask: Option<&[signal::Number]>,
    ) -> Result<c_int> {
        (&mut &*self).select(readers, writers, timeout, signal_mask)
    }
    #[inline]
    fn getpid(&self) -> Pid {
        (&self).getpid()
    }
    #[inline]
    fn getppid(&self) -> Pid {
        (&self).getppid()
    }
    #[inline]
    fn getpgrp(&self) -> Pid {
        (&self).getpgrp()
    }
    #[inline]
    fn setpgid(&mut self, pid: Pid, pgid: Pid) -> Result<()> {
        (&mut &*self).setpgid(pid, pgid)
    }
    #[inline]
    fn tcgetpgrp(&self, fd: Fd) -> Result<Pid> {
        (&self).tcgetpgrp(fd)
    }
    #[inline]
    fn tcsetpgrp(&mut self, fd: Fd, pgid: Pid) -> Result<()> {
        (&mut &*self).tcsetpgrp(fd, pgid)
    }
    #[inline]
    fn new_child_process(&mut self) -> Result<ChildProcessStarter> {
        (&mut &*self).new_child_process()
    }
    #[inline]
    fn wait(&mut self, target: Pid) -> Result<Option<(Pid, ProcessState)>> {
        (&mut &*self).wait(target)
    }
    #[inline]
    fn execve(&mut self, path: &CStr, args: &[CString], envs: &[CString]) -> Result<Infallible> {
        (&mut &*self).execve(path, args, envs)
    }
    #[inline]
    fn getcwd(&self) -> Result<PathBuf> {
        (&self).getcwd()
    }
    #[inline]
    fn chdir(&mut self, path: &CStr) -> Result<()> {
        (&mut &*self).chdir(path)
    }
    #[inline]
    fn getuid(&self) -> Uid {
        (&self).getuid()
    }
    #[inline]
    fn geteuid(&self) -> Uid {
        (&self).geteuid()
    }
    #[inline]
    fn getgid(&self) -> Gid {
        (&self).getgid()
    }
    #[inline]
    fn getegid(&self) -> Gid {
        (&self).getegid()
    }
    #[inline]
    fn getpwnam_dir(&self, name: &str) -> Result<Option<PathBuf>> {
        (&self).getpwnam_dir(name)
    }
    #[inline]
    fn confstr_path(&self) -> Result<OsString> {
        (&self).confstr_path()
    }
    #[inline]
    fn shell_path(&self) -> CString {
        (&self).shell_path()
    }
    #[inline]
    fn getrlimit(&self, resource: Resource) -> std::io::Result<LimitPair> {
        (&self).getrlimit(resource)
    }
    #[inline]
    fn setrlimit(&mut self, resource: Resource, limits: LimitPair) -> std::io::Result<()> {
        (&mut &*self).setrlimit(resource, limits)
    }
}

impl SignalSystem for &SharedSystem {
    #[inline]
    fn signal_name_from_number(&self, number: signal::Number) -> signal::Name {
        SystemEx::signal_name_from_number(*self, number)
    }

    #[inline]
    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number> {
        System::signal_number_from_name(*self, name)
    }

    fn set_signal_handling(
        &mut self,
        signal: signal::Number,
        handling: SignalHandling,
    ) -> Result<SignalHandling> {
        self.0.borrow_mut().set_signal_handling(signal, handling)
    }
}

impl SignalSystem for SharedSystem {
    #[inline]
    fn signal_name_from_number(&self, number: signal::Number) -> signal::Name {
        SystemEx::signal_name_from_number(self, number)
    }

    #[inline]
    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number> {
        System::signal_number_from_name(self, name)
    }

    #[inline]
    fn set_signal_handling(
        &mut self,
        signal: signal::Number,
        handling: SignalHandling,
    ) -> Result<SignalHandling> {
        self.0.borrow_mut().set_signal_handling(signal, handling)
    }
}

#[cfg(test)]
mod tests {
    use super::super::r#virtual::VirtualSystem;
    use super::super::r#virtual::PIPE_SIZE;
    use super::super::r#virtual::{SIGCHLD, SIGINT, SIGTERM, SIGUSR1};
    use super::*;
    use assert_matches::assert_matches;
    use futures_util::task::noop_waker_ref;
    use futures_util::FutureExt as _;
    use std::task::Context;
    use std::task::Poll;
    use std::time::Duration;

    #[test]
    fn shared_system_read_async_ready() {
        let mut system = SharedSystem::new(Box::new(VirtualSystem::new()));
        let (reader, writer) = system.pipe().unwrap();
        system.write(writer, &[42]).unwrap();

        let mut buffer = [0; 2];
        let result = system.read_async(reader, &mut buffer).now_or_never();
        assert_eq!(result, Some(Ok(1)));
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

        let result = system2.select(false);
        assert_eq!(result, Ok(()));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        state.borrow_mut().processes[&process_id].fds[&writer]
            .open_file_description
            .borrow_mut()
            .write(&[56])
            .unwrap();

        let result = future.as_mut().poll(&mut context);
        drop(future);
        assert_eq!(result, Poll::Ready(Ok(1)));
        assert_eq!(buffer[..1], [56]);
    }

    #[test]
    fn shared_system_write_all_ready() {
        let mut system = SharedSystem::new(Box::new(VirtualSystem::new()));
        let (reader, writer) = system.pipe().unwrap();
        let result = system.write_all(writer, &[17]).now_or_never().unwrap();
        assert_eq!(result, Ok(1));

        let mut buffer = [0; 2];
        system.read(reader, &mut buffer).unwrap();
        assert_eq!(buffer[..1], [17]);
    }

    #[test]
    fn shared_system_write_all_not_ready_at_first() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        let (reader, writer) = system.pipe().unwrap();

        state.borrow_mut().processes[&process_id].fds[&writer]
            .open_file_description
            .borrow_mut()
            .write(&[42; PIPE_SIZE])
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut out_buffer = [87; PIPE_SIZE];
        out_buffer[0] = 0;
        out_buffer[1] = 1;
        out_buffer[PIPE_SIZE - 2] = 0xFE;
        out_buffer[PIPE_SIZE - 1] = 0xFF;
        let mut future = Box::pin(system.write_all(writer, &out_buffer));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        let mut in_buffer = [0; PIPE_SIZE - 1];
        state.borrow_mut().processes[&process_id].fds[&reader]
            .open_file_description
            .borrow_mut()
            .read(&mut in_buffer)
            .unwrap();
        assert_eq!(in_buffer, [42; PIPE_SIZE - 1]);

        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        in_buffer[0] = 0;
        state.borrow_mut().processes[&process_id].fds[&reader]
            .open_file_description
            .borrow_mut()
            .read(&mut in_buffer[..1])
            .unwrap();
        assert_eq!(in_buffer[..1], [42; 1]);

        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Ready(Ok(out_buffer.len())));

        state.borrow_mut().processes[&process_id].fds[&reader]
            .open_file_description
            .borrow_mut()
            .read(&mut in_buffer)
            .unwrap();
        assert_eq!(in_buffer, out_buffer[..PIPE_SIZE - 1]);
        state.borrow_mut().processes[&process_id].fds[&reader]
            .open_file_description
            .borrow_mut()
            .read(&mut in_buffer)
            .unwrap();
        assert_eq!(in_buffer[..1], out_buffer[PIPE_SIZE - 1..]);
    }

    #[test]
    fn shared_system_write_all_empty() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        let (_reader, writer) = system.pipe().unwrap();

        state.borrow_mut().processes[&process_id].fds[&writer]
            .open_file_description
            .borrow_mut()
            .write(&[0; PIPE_SIZE])
            .unwrap();

        // Even if the pipe is full, empty write succeeds.
        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.write_all(writer, &[]));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Ready(Ok(0)));
        // TODO Make sure `write` is not called at all
    }

    // TODO Test SharedSystem::write_all where second write returns EINTR

    #[test]
    fn shared_system_wait_until() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let system = SharedSystem::new(Box::new(system));
        let start = Instant::now();
        state.borrow_mut().now = Some(start);
        let target = start + Duration::from_millis(1_125);

        let mut future = Box::pin(system.wait_until(target));
        let mut context = Context::from_waker(noop_waker_ref());
        let poll = future.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);

        system.select(false).unwrap();
        let poll = future.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Ready(()));
        assert_eq!(state.borrow().now, Some(target));
    }

    #[test]
    fn shared_system_wait_for_signals() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        system
            .set_signal_handling(SIGCHLD, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(SIGINT, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(SIGUSR1, SignalHandling::Catch)
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.wait_for_signals());
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            assert!(process.blocked_signals().contains(&SIGCHLD));
            assert!(process.blocked_signals().contains(&SIGINT));
            assert!(process.blocked_signals().contains(&SIGUSR1));
            let _ = process.raise_signal(SIGCHLD);
            let _ = process.raise_signal(SIGINT);
        }
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        system.select(false).unwrap();
        let result = future.as_mut().poll(&mut context);
        assert_matches!(result, Poll::Ready(signals) => {
            assert_eq!(signals.len(), 2);
            assert!(signals.contains(&SIGCHLD));
            assert!(signals.contains(&SIGINT));
        });
    }

    #[test]
    fn shared_system_wait_for_signal_returns_on_caught() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        system
            .set_signal_handling(SIGCHLD, SignalHandling::Catch)
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.wait_for_signal(SIGCHLD));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            assert!(process.blocked_signals().contains(&SIGCHLD));
            let _ = process.raise_signal(SIGCHLD);
        }
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        system.select(false).unwrap();
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Ready(()));
    }

    #[test]
    fn shared_system_wait_for_signal_ignores_irrelevant_signals() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        system
            .set_signal_handling(SIGINT, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(SIGTERM, SignalHandling::Catch)
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.wait_for_signal(SIGINT));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            let _ = process.raise_signal(SIGCHLD);
            let _ = process.raise_signal(SIGTERM);
        }
        system.select(false).unwrap();

        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);
    }

    #[test]
    fn shared_system_select_consumes_all_pending_signals() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        system
            .set_signal_handling(SIGINT, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(SIGTERM, SignalHandling::Catch)
            .unwrap();

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            let _ = process.raise_signal(SIGINT);
            let _ = process.raise_signal(SIGTERM);
        }
        system.select(false).unwrap();

        let state = state.borrow();
        let process = state.processes.get(&process_id).unwrap();
        let blocked = process.blocked_signals();
        assert!(blocked.contains(&SIGINT));
        assert!(blocked.contains(&SIGTERM));
        let pending = process.pending_signals();
        assert!(!pending.contains(&SIGINT));
        assert!(!pending.contains(&SIGTERM));
    }

    #[test]
    fn shared_system_select_does_not_wake_signal_waiters_on_io() {
        let system = VirtualSystem::new();
        let mut system_1 = SharedSystem::new(Box::new(system));
        let mut system_2 = system_1.clone();
        let mut system_3 = system_1.clone();
        let (reader, writer) = system_1.pipe().unwrap();
        system_2
            .set_signal_handling(SIGCHLD, SignalHandling::Catch)
            .unwrap();

        let mut buffer = [0];
        let mut read_future = Box::pin(system_1.read_async(reader, &mut buffer));
        let mut signal_future = Box::pin(system_2.wait_for_signals());
        let mut context = Context::from_waker(noop_waker_ref());
        let result = read_future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);
        let result = signal_future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);
        system_3.write(writer, &[42]).unwrap();
        system_3.select(false).unwrap();

        let result = read_future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Ready(Ok(1)));
        let result = signal_future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);
    }

    #[test]
    fn shared_system_select_poll() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let system = SharedSystem::new(Box::new(system));
        let start = Instant::now();
        state.borrow_mut().now = Some(start);
        let target = start + Duration::from_millis(1_125);

        let mut future = Box::pin(system.wait_until(target));
        let mut context = Context::from_waker(noop_waker_ref());
        let poll = future.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);

        system.select(true).unwrap();
        let poll = future.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert_eq!(state.borrow().now, Some(start));
    }
}
