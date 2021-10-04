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

//! [System] and related types.

use crate::io::Fd;
use crate::Env;
use async_trait::async_trait;
use futures_util::future::poll_fn;
use futures_util::task::Poll;
use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::sys::select::FdSet;
use nix::sys::signal::SigSet;
use nix::sys::signal::SigmaskHow;
use nix::sys::signal::Signal;
use nix::sys::stat::Mode;
use nix::sys::wait::WaitStatus;
use nix::unistd::Pid;
use std::cell::RefCell;
use std::convert::Infallible;
use std::ffi::CStr;
use std::ffi::CString;
use std::fmt::Debug;
use std::future::Future;
use std::ops::Deref;
use std::ops::DerefMut;
use std::os::raw::c_int;
use std::pin::Pin;
use std::rc::Rc;
use std::rc::Weak;
use std::task::Waker;

/// API to the system-managed parts of the environment.
///
/// The `System` trait defines a collection of methods to access the underlying
/// operating system from the shell as an application program. There are two
/// substantial implementors for this trait:
/// [`RealSystem`](crate::real_system::RealSystem) and
/// [`VirtualSystem`](crate::virtual_system::VirtualSystem). Another implementor
/// is [`SharedSystem`], which wraps a `System` instance to extend the interface
/// with asynchronous methods.
pub trait System: Debug {
    /// Whether there is an executable file at the specified path.
    fn is_executable_file(&self, path: &CStr) -> bool;

    /// Creates an unnamed pipe.
    ///
    /// This is a thin wrapper around the `pipe` system call.
    /// If successful, returns the reading and writing ends of the pipe.
    fn pipe(&mut self) -> nix::Result<(Fd, Fd)>;

    /// Duplicates a file descriptor.
    ///
    /// This is a thin wrapper around the `fcntl` system call that opens a new
    /// FD that shares the open file description with `from`. The new FD will be
    /// the minimum unused FD not less than `to_min`.  The `cloexec` parameter
    /// specifies whether the new FD should have the `CLOEXEC` flag set. If
    /// successful, returns `Ok(new_fd)`. On error, returns `Err(_)`.
    fn dup(&mut self, from: Fd, to_min: Fd, cloexec: bool) -> nix::Result<Fd>;

    /// Duplicates a file descriptor.
    ///
    /// This is a thin wrapper around the `dup2` system call. If successful,
    /// returns `Ok(to)`. On error, returns `Err(_)`.
    fn dup2(&mut self, from: Fd, to: Fd) -> nix::Result<Fd>;

    /// Opens a file descriptor.
    ///
    /// This is a thin wrapper around the `open` system call.
    fn open(&mut self, path: &CStr, option: OFlag, mode: Mode) -> nix::Result<Fd>;

    /// Closes a file descriptor.
    ///
    /// This is a thin wrapper around the `close` system call.
    ///
    /// This function returns `Ok(())` when the FD is already closed.
    fn close(&mut self, fd: Fd) -> nix::Result<()>;

    /// Returns the file status flags for the file descriptor.
    fn fcntl_getfl(&self, fd: Fd) -> nix::Result<OFlag>;

    /// Sets the file status flags for the file descriptor.
    fn fcntl_setfl(&mut self, fd: Fd, flags: OFlag) -> nix::Result<()>;

    /// Reads from the file descriptor.
    ///
    /// This is a thin wrapper around the `read` system call.
    /// If successful, returns the number of bytes read.
    ///
    /// This function may perform blocking I/O, especially if the `O_NONBLOCK`
    /// flag is not set for the FD. Use [`SharedSystem::read_async`] to support
    /// concurrent I/O in an `async` function context.
    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> nix::Result<usize>;

    /// Writes to the file descriptor.
    ///
    /// This is a thin wrapper around the `write` system call.
    /// If successful, returns the number of bytes written.
    ///
    /// This function may write only part of the `buffer` and block if the
    /// `O_NONBLOCK` flag is not set for the FD. Use [`SharedSystem::write_all`]
    /// to support concurrent I/O in an `async` function context and ensure the
    /// whole `buffer` is written.
    fn write(&mut self, fd: Fd, buffer: &[u8]) -> nix::Result<usize>;

    /// Gets and/or sets the signal blocking mask.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::set_signal_handling`]. You should not call this function
    /// directly, or you will disrupt the behavior of `SharedSystem`. The
    /// description below applies if you want to do everything yourself without
    /// depending on `SharedSystem`.
    ///
    /// This is a thin wrapper around the `sigprocmask` system call. If `set` is
    /// `Some`, this function updates the signal blocking mask according to
    /// `how`. If `oldset` is `Some`, this function sets the previous mask to
    /// it.
    fn sigmask(
        &mut self,
        how: SigmaskHow,
        set: Option<&SigSet>,
        oldset: Option<&mut SigSet>,
    ) -> nix::Result<()>;

    /// Gets and sets the handler for a signal.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::set_signal_handling`]. You should not call this function
    /// directly, or you will disrupt the behavior of `SharedSystem`. The
    /// description below applies if you want to do everything yourself without
    /// depending on `SharedSystem`.
    ///
    /// This is an abstract wrapper around the `sigaction` system call. This
    /// function returns the previous handler if successful.
    ///
    /// When you set the handler to `SignalHandling::Catch`, signals sent to
    /// this process are accumulated in the `System` instance and made available
    /// from [`caught_signals`](Self::caught_signals).
    fn sigaction(&mut self, signal: Signal, action: SignalHandling) -> nix::Result<SignalHandling>;

    /// Returns signals this process has caught, if any.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::select`]. You should not call this function directly, or
    /// you will disrupt the behavior of `SharedSystem`. The description below
    /// applies if you want to do everything yourself without depending on
    /// `SharedSystem`.
    ///
    /// To catch a signal, you must set the signal handler to
    /// [`SignalHandling::Catch`] by calling [`sigaction`](Self::sigaction)
    /// first. Once the handler is ready, signals sent to the process are
    /// accumulated in the `System`. You call `caught_signals` to obtain a list
    /// of caught signals thus far.
    ///
    /// This function clears the internal list of caught signals, so a next call
    /// will return an empty list unless another signal is caught since the
    /// first call. Because the list size is limited, you should call this
    /// function periodically before the list gets full, in which case further
    /// caught signals are silently ignored.
    ///
    /// Note that signals become pending if sent while blocked by
    /// [`sigmask`](Self::sigmask). They must be unblocked so that they are
    /// caught and made available from this function.
    fn caught_signals(&mut self) -> Vec<Signal>;

    // TODO timespec
    /// Waits for a next event.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::select`]. You should not call this function directly, or
    /// you will disrupt the behavior of `SharedSystem`. The description below
    /// applies if you want to do everything yourself without depending on
    /// `SharedSystem`.
    ///
    /// This function blocks the calling thread until one of the following
    /// condition is met:
    ///
    /// - An FD in `readers` becomes ready for reading.
    /// - An FD in `writers` becomes ready for writing.
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
    /// If `signal_mask` is `Some` signal set, the signal blocking mask is set
    /// to it while waiting and restored when the function returns.
    fn select(
        &mut self,
        readers: &mut FdSet,
        writers: &mut FdSet,
        signal_mask: Option<&SigSet>,
    ) -> nix::Result<c_int>;

    /// Creates a new child process.
    ///
    /// This is a thin wrapper around the `fork` system call. Users of `Env`
    /// should not call it directly. Instead, use [`Env::run_in_subshell`] so
    /// that the environment can manage the created child process as a job
    /// member.
    ///
    /// If successful, this function returns a [`ChildProcess`] object. The
    /// caller must call [`ChildProcess::run`] exactly once so that the child
    /// process performs its task and finally exit.
    ///
    /// This function does not return any information about whether the current
    /// process is the original (parent) process or the new (child) process. The
    /// caller does not have to (and should not) care that because
    /// `ChildProcess::run` takes care of it.
    fn new_child_process(&mut self) -> nix::Result<Box<dyn ChildProcess>>;

    /// Reports updated status of a child process.
    ///
    /// This is a low-level function used internally by
    /// [`Env::wait_for_subshell`]. You should not call this function directly,
    /// or you will disrupt the behavior of `Env`. The description below applies
    /// if you want to do everything yourself without depending on `Env`.
    ///
    /// This function performs
    /// `waitpid(-1, ..., WUNTRACED | WCONTINUED | WNOHANG)`.
    /// Despite the name, this function does not block: it returns the result
    /// immediately.
    fn wait(&mut self) -> nix::Result<nix::sys::wait::WaitStatus>;

    // TODO Consider passing raw pointers for optimization
    /// Replaces the current process with an external utility.
    ///
    /// This is a thin wrapper around the `execve` system call.
    fn execve(
        &mut self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> nix::Result<Infallible>;
}

/// How to handle a signal.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SignalHandling {
    /// Perform the default action for the signal.
    Default,
    /// Ignore the signal.
    Ignore,
    /// Catch the signal.
    Catch,
}

impl Default for SignalHandling {
    fn default() -> Self {
        SignalHandling::Default
    }
}

/// Type of an argument to [`ChildProcess::run`].
pub type ChildProcessTask =
    Box<dyn for<'a> FnMut(&'a mut Env) -> Pin<Box<dyn Future<Output = ()> + 'a>>>;

/// Abstraction of a child process that can run a task.
///
/// [`System::new_child_process`] returns an implementor of `ChildProcess`. You
/// must call [`run`](Self::run) exactly once.
#[async_trait(?Send)]
pub trait ChildProcess: Debug {
    /// Runs a task in the child process.
    ///
    /// When called in the parent process, this function returns the process ID
    /// of the child. When in the child, this function never returns.
    async fn run(&mut self, env: &mut Env, task: ChildProcessTask) -> Pid;
    // TODO When unsized_fn_params is stabilized,
    // 1. `&mut self` should be `self`
    // 2. `task` should be `FnOnce` rather than `FnMut`
}

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
/// system3.select().unwrap();
///
/// // Now the read task can proceed to the end.
/// let number = executor.run_until(read_task.unwrap());
/// assert_eq!(number, 123);
/// ```
///
/// If there is a child process in the [`VirtualSystem`](crate::VirtualSystem),
/// you should call
/// [`SystemState::select_all`](crate::virtual_system::SystemState) in addition
/// to [`SharedSystem::select`](crate::SharedSystem) so that the child process
/// task is woken up when needed.
/// (TBD code example)
#[derive(Clone, Debug)]
pub struct SharedSystem(pub(crate) Rc<RefCell<SelectSystem>>);

impl SharedSystem {
    /// Creates a new shared system.
    pub fn new(system: Box<dyn System>) -> Self {
        SharedSystem(Rc::new(RefCell::new(SelectSystem::new(system))))
    }

    fn set_nonblocking(&mut self, fd: Fd) -> nix::Result<OFlag> {
        let mut inner = self.0.borrow_mut();
        let flags = inner.system.fcntl_getfl(fd)?;
        if !flags.contains(OFlag::O_NONBLOCK) {
            inner.system.fcntl_setfl(fd, flags | OFlag::O_NONBLOCK)?;
        }
        Ok(flags)
    }

    fn reset_nonblocking(&mut self, fd: Fd, old_flags: OFlag) {
        if !old_flags.contains(OFlag::O_NONBLOCK) {
            let _: Result<(), _> = self.0.borrow_mut().system.fcntl_setfl(fd, old_flags);
        }
    }

    /// Reads from the file descriptor.
    ///
    /// This function waits for one or more bytes to be available for reading.
    /// If successful, returns the number of bytes read.
    pub async fn read_async(&mut self, fd: Fd, buffer: &mut [u8]) -> nix::Result<usize> {
        let flags = self.set_nonblocking(fd)?;

        let result = poll_fn(|context| {
            let mut inner = self.0.borrow_mut();
            match inner.system.read(fd, buffer) {
                Err(Errno::EAGAIN) => {
                    inner.io.wait_for_reading(fd, context.waker().clone());
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
    pub async fn write_all(&mut self, fd: Fd, mut buffer: &[u8]) -> nix::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }

        let flags = self.set_nonblocking(fd)?;
        let mut written = 0;

        let result = poll_fn(|context| {
            let mut inner = self.0.borrow_mut();
            match inner.system.write(fd, buffer) {
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
            inner.io.wait_for_writing(fd, context.waker().clone());
            Poll::Pending
        })
        .await;

        self.reset_nonblocking(fd, flags);

        result
    }

    /// Sets how a signal is handled.
    ///
    /// This function updates the signal blocking mask and the signal action for
    /// the specified signal and remembers the previous configuration for
    /// restoration.
    ///
    /// Returns the previous handler.
    pub fn set_signal_handling(
        &mut self,
        signal: Signal,
        handling: SignalHandling,
    ) -> nix::Result<SignalHandling> {
        self.0.borrow_mut().set_signal_handling(signal, handling)
    }

    /// Waits for a signal to be delivered to this process.
    ///
    /// Before calling this function, you need to [set signal
    /// handling](Self::set_signal_handling) to `Catch`.
    /// Without doing so, this function cannot detect the receipt of the signal.
    pub async fn wait_for_signal(&self, signal: Signal) {
        let awaiter = self.0.borrow_mut().signal.wait_for_signal(signal);
        poll_fn(|context| {
            let mut awaiter = awaiter.borrow_mut();
            match awaiter.status {
                SignalStatus::Caught => Poll::Ready(()),
                SignalStatus::Expected(ref mut waker) => {
                    *waker = Some(context.waker().clone());
                    Poll::Pending
                }
            }
        })
        .await
    }

    /// Waits for a next event to occur.
    ///
    /// This function calls [`System::select`] with arguments computed from the
    /// current internal state of the `SharedSystem`. It will wake up tasks
    /// waiting for the file descriptor to be ready in
    /// [`read_async`](Self::read_async) and [`write_all`](Self::write_all) or
    /// for a signal to be caught in [`wait_for_signal`](Self::wait_for_signal).
    ///
    /// This function may wake up a task even if the condition it is expecting
    /// has not yet been met.
    pub fn select(&self) -> nix::Result<()> {
        self.0.borrow_mut().select()
    }
}

impl System for SharedSystem {
    fn is_executable_file(&self, path: &CStr) -> bool {
        self.0.borrow().is_executable_file(path)
    }
    fn pipe(&mut self) -> nix::Result<(Fd, Fd)> {
        self.0.borrow_mut().pipe()
    }
    fn dup(&mut self, from: Fd, to_min: Fd, cloexec: bool) -> nix::Result<Fd> {
        self.0.borrow_mut().dup(from, to_min, cloexec)
    }
    fn dup2(&mut self, from: Fd, to: Fd) -> nix::Result<Fd> {
        self.0.borrow_mut().dup2(from, to)
    }
    fn open(&mut self, path: &CStr, option: OFlag, mode: Mode) -> nix::Result<Fd> {
        self.0.borrow_mut().open(path, option, mode)
    }
    fn close(&mut self, fd: Fd) -> nix::Result<()> {
        self.0.borrow_mut().close(fd)
    }
    fn fcntl_getfl(&self, fd: Fd) -> nix::Result<OFlag> {
        self.0.borrow().fcntl_getfl(fd)
    }
    fn fcntl_setfl(&mut self, fd: Fd, flags: OFlag) -> nix::Result<()> {
        self.0.borrow_mut().fcntl_setfl(fd, flags)
    }
    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> nix::Result<usize> {
        self.0.borrow_mut().read(fd, buffer)
    }
    fn write(&mut self, fd: Fd, buffer: &[u8]) -> nix::Result<usize> {
        self.0.borrow_mut().write(fd, buffer)
    }
    fn sigmask(
        &mut self,
        how: SigmaskHow,
        set: Option<&SigSet>,
        oldset: Option<&mut SigSet>,
    ) -> nix::Result<()> {
        self.0.borrow_mut().sigmask(how, set, oldset)
    }
    fn sigaction(&mut self, signal: Signal, action: SignalHandling) -> nix::Result<SignalHandling> {
        self.0.borrow_mut().sigaction(signal, action)
    }
    fn caught_signals(&mut self) -> Vec<Signal> {
        self.0.borrow_mut().caught_signals()
    }
    fn select(
        &mut self,
        readers: &mut FdSet,
        writers: &mut FdSet,
        signal_mask: Option<&SigSet>,
    ) -> nix::Result<c_int> {
        (**self.0.borrow_mut()).select(readers, writers, signal_mask)
    }
    fn new_child_process(&mut self) -> nix::Result<Box<dyn ChildProcess>> {
        self.0.borrow_mut().new_child_process()
    }
    fn wait(&mut self) -> nix::Result<WaitStatus> {
        self.0.borrow_mut().wait()
    }
    fn execve(
        &mut self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> nix::Result<Infallible> {
        self.0.borrow_mut().execve(path, args, envs)
    }
}

/// [System] extended with internal state to support asynchronous functions.
///
/// A `SelectSystem` is a container of a `System` and internal data a
/// [`SharedSystem`] uses to implement asynchronous I/O, signal handling, and
/// timer function. The contained `System` can be accessed via the `Deref` and
/// `DerefMut` implementations.
///
/// TODO Elaborate
#[derive(Debug)]
pub(crate) struct SelectSystem {
    system: Box<dyn System>,
    io: AsyncIo,
    signal: AsyncSignal,
    current_mask: Option<SigSet>,
}

impl Deref for SelectSystem {
    type Target = Box<dyn System>;
    fn deref(&self) -> &Box<dyn System> {
        &self.system
    }
}

impl DerefMut for SelectSystem {
    fn deref_mut(&mut self) -> &mut Box<dyn System> {
        &mut self.system
    }
}

impl SelectSystem {
    /// Creates a new `SelectSystem` that wraps the given `System`.
    pub fn new(system: Box<dyn System>) -> Self {
        SelectSystem {
            system,
            io: AsyncIo::new(),
            signal: AsyncSignal::new(),
            current_mask: None,
        }
    }

    /// Implements signal handler update.
    ///
    /// See [`SharedSystem::set_signal_handling`].
    pub fn set_signal_handling(
        &mut self,
        signal: Signal,
        handling: SignalHandling,
    ) -> nix::Result<SignalHandling> {
        let mut set = SigSet::empty();
        let mut old_set = SigSet::empty();
        set.add(signal);

        // The order of sigmask and sigaction is important to prevent the signal
        // from being caught. The signal must be caught only when the select
        // function temporarily unblocks the signal. This is to avoid race
        // condition.
        // TODO Remember the previous settings for restoration
        match handling {
            SignalHandling::Default | SignalHandling::Ignore => {
                let old_handling = self.system.sigaction(signal, handling)?;
                self.system
                    .sigmask(SigmaskHow::SIG_UNBLOCK, Some(&set), Some(&mut old_set))?;
                old_set.remove(signal);
                self.current_mask = Some(old_set);
                Ok(old_handling)
            }
            SignalHandling::Catch => {
                self.system
                    .sigmask(SigmaskHow::SIG_BLOCK, Some(&set), Some(&mut old_set))?;
                old_set.add(signal);
                self.current_mask = Some(old_set);
                self.system.sigaction(signal, handling)
            }
        }
    }

    /// Implements the select function for `SharedSystem`.
    ///
    /// See [`SharedSystem::select`].
    pub fn select(&mut self) -> nix::Result<()> {
        let mut readers = self.io.readers();
        let mut writers = self.io.writers();
        let mut mask = self.current_mask;
        if let Some(mask) = &mut mask {
            for signal in self.signal.expected_signals() {
                mask.remove(signal);
            }
        }

        match self
            .system
            .select(&mut readers, &mut writers, mask.as_ref())
        {
            Ok(_) => {
                self.io.wake(readers, writers);
                self.signal.wake(&to_sig_set(self.system.caught_signals()));
                Ok(())
            }
            Err(Errno::EBADF) => {
                // Some of the readers and writers are invalid but we cannot
                // tell which, so we wake up everything.
                self.io.wake_all();
                Err(Errno::EBADF)
            }
            Err(Errno::EINTR) => {
                self.signal.wake(&to_sig_set(self.system.caught_signals()));
                Ok(())
            }
            Err(error) => Err(error),
        }
        // TODO Support timers
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
    readers: Vec<FdAwaiter>,
    writers: Vec<FdAwaiter>,
}

#[derive(Clone, Debug)]
struct FdAwaiter {
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
        self.readers.push(FdAwaiter { fd, waker });
    }

    /// Adds an awaiter for writing.
    pub fn wait_for_writing(&mut self, fd: Fd, waker: Waker) {
        self.writers.push(FdAwaiter { fd, waker });
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

/// Helper for `select`ing on signals.
///
/// An `AsyncSignal` is a set of [Waker]s that are waiting for a signal to be
/// caught by the current process.
///
/// TODO Elaborate
#[derive(Clone, Debug, Default)]
struct AsyncSignal {
    awaiters: Vec<Weak<RefCell<SignalAwaiter>>>,
}

#[derive(Clone, Debug)]
struct SignalAwaiter {
    signal: Signal,
    status: SignalStatus,
}

#[derive(Clone, Debug)]
enum SignalStatus {
    Expected(Option<Waker>),
    Caught,
}

impl AsyncSignal {
    /// Returns a new empty `AsyncSignal`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns an iterator of signals being waited for.
    pub fn expected_signals(&self) -> impl Iterator<Item = Signal> + '_ {
        self.awaiters
            .iter()
            .filter_map(|awaiter| awaiter.upgrade().map(|awaiter| awaiter.borrow().signal))
    }

    /// Adds an awaiter for a signal.
    ///
    /// This function returns a reference-counted `SignalAwaiter`.
    /// The caller must set a waker to the returned `SignalAwaiter`. When the
    /// signal is caught, the waker is woken and removed from the
    /// `SignalAwaiter`. The caller can replace the waker in the `SignalAwaiter`
    /// with another if the previous waker gets expired.
    pub fn wait_for_signal(&mut self, signal: Signal) -> Rc<RefCell<SignalAwaiter>> {
        let status = SignalStatus::Expected(None);
        let awaiter = Rc::new(RefCell::new(SignalAwaiter { signal, status }));
        self.awaiters.push(Rc::downgrade(&awaiter));
        awaiter
    }

    /// Wakes awaiters for caught signals.
    ///
    /// This function wakes up awaiters for the signals contained in the
    /// argument. Once woken, the awaiters are removed from `self`.
    ///
    /// This function borrows `SignalAwaiter`s returned from `wait_for_signal`
    /// so you must not have conflicting borrows.
    pub fn wake(&mut self, signals: &SigSet) {
        // TODO use Vec::drain_filter
        for i in (0..self.awaiters.len()).rev() {
            let remove = if let Some(awaiter) = self.awaiters[i].upgrade() {
                let mut awaiter = awaiter.borrow_mut();
                if signals.contains(awaiter.signal) {
                    if let SignalStatus::Expected(Some(waker)) =
                        std::mem::replace(&mut awaiter.status, SignalStatus::Caught)
                    {
                        waker.wake()
                    }
                    true
                } else {
                    false
                }
            } else {
                true
            };
            if remove {
                self.awaiters.remove(i);
            }
        }
    }
}

/// Converts a signal collection into a `SigSet`.
fn to_sig_set<I: IntoIterator<Item = Signal>>(i: I) -> SigSet {
    let mut signals = SigSet::empty();
    for signal in i {
        signals.add(signal);
    }
    signals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virtual_system::Pipe;
    use crate::virtual_system::VirtualSystem;
    use assert_matches::assert_matches;
    use futures_executor::block_on;
    use futures_util::task::noop_waker;
    use futures_util::task::noop_waker_ref;
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
        let result = block_on(system.write_all(writer, &[17]));
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
            .write(&[42; Pipe::PIPE_SIZE])
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut out_buffer = [87; Pipe::PIPE_SIZE];
        out_buffer[0] = 0;
        out_buffer[1] = 1;
        out_buffer[Pipe::PIPE_SIZE - 2] = 0xFE;
        out_buffer[Pipe::PIPE_SIZE - 1] = 0xFF;
        let mut future = Box::pin(system.write_all(writer, &out_buffer));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        let mut in_buffer = [0; Pipe::PIPE_SIZE - 1];
        state.borrow_mut().processes[&process_id].fds[&reader]
            .open_file_description
            .borrow_mut()
            .read(&mut in_buffer)
            .unwrap();
        assert_eq!(in_buffer, [42; Pipe::PIPE_SIZE - 1]);

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
        assert_eq!(in_buffer, out_buffer[..Pipe::PIPE_SIZE - 1]);
        state.borrow_mut().processes[&process_id].fds[&reader]
            .open_file_description
            .borrow_mut()
            .read(&mut in_buffer)
            .unwrap();
        assert_eq!(in_buffer[..1], out_buffer[Pipe::PIPE_SIZE - 1..]);
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
            .write(&[0; Pipe::PIPE_SIZE])
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
    fn shared_system_wait_for_signal() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        system
            .set_signal_handling(Signal::SIGCHLD, SignalHandling::Catch)
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.wait_for_signal(Signal::SIGCHLD));
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            assert!(process.blocked_signals().contains(Signal::SIGCHLD));
            process.raise_signal(Signal::SIGCHLD);
        }
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);

        system.select().unwrap();
        let result = future.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Ready(()));
    }

    #[test]
    fn shared_system_select_leaves_irrelevant_signal_pending() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut system = SharedSystem::new(Box::new(system));
        system
            .set_signal_handling(Signal::SIGINT, SignalHandling::Catch)
            .unwrap();
        system
            .set_signal_handling(Signal::SIGTERM, SignalHandling::Catch)
            .unwrap();

        let mut context = Context::from_waker(noop_waker_ref());
        let mut future = Box::pin(system.wait_for_signal(Signal::SIGINT));
        let _ = future.as_mut().poll(&mut context);
        {
            let mut state = state.borrow_mut();
            let process = state.processes.get_mut(&process_id).unwrap();
            process.raise_signal(Signal::SIGINT);
            process.raise_signal(Signal::SIGTERM);
        }
        system.select().unwrap();

        let state = state.borrow();
        let process = state.processes.get(&process_id).unwrap();
        let blocked = process.blocked_signals();
        assert!(blocked.contains(Signal::SIGINT));
        assert!(blocked.contains(Signal::SIGTERM));
        let pending = process.pending_signals();
        assert!(!pending.contains(Signal::SIGINT));
        assert!(pending.contains(Signal::SIGTERM));
    }

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

    #[test]
    fn async_signal_has_no_default_expected_signals() {
        let async_signal = AsyncSignal::new();
        let signals = async_signal.expected_signals().collect::<Vec<Signal>>();
        assert_eq!(signals, []);
    }

    #[test]
    fn async_signal_with_some_expected_signals() {
        let mut async_signal = AsyncSignal::new();
        let awaiter_1 = async_signal.wait_for_signal(Signal::SIGCHLD);
        let awaiter_2 = async_signal.wait_for_signal(Signal::SIGINT);
        let awaiter_1 = awaiter_1.borrow();
        assert_eq!(awaiter_1.signal, Signal::SIGCHLD);
        assert_matches!(awaiter_1.status, SignalStatus::Expected(None));
        let awaiter_2 = awaiter_2.borrow();
        assert_eq!(awaiter_2.signal, Signal::SIGINT);
        assert_matches!(awaiter_2.status, SignalStatus::Expected(None));

        let signals = async_signal.expected_signals().collect::<Vec<Signal>>();
        assert_eq!(signals, [Signal::SIGCHLD, Signal::SIGINT]);
    }

    #[test]
    fn async_signal_wake() {
        let mut async_signal = AsyncSignal::new();
        let awaiter_1 = async_signal.wait_for_signal(Signal::SIGCHLD);
        awaiter_1.borrow_mut().status = SignalStatus::Expected(Some(noop_waker()));

        async_signal.wake(&SigSet::empty());
        assert_matches!(awaiter_1.borrow().status, SignalStatus::Expected(Some(_)));
        let signals = async_signal.expected_signals().collect::<Vec<Signal>>();
        assert_eq!(signals, [Signal::SIGCHLD]);

        async_signal.wake(&SigSet::all());
        assert_matches!(awaiter_1.borrow().status, SignalStatus::Caught);
        let signals = async_signal.expected_signals().collect::<Vec<Signal>>();
        assert_eq!(signals, []);
    }
}
