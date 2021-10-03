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

//! This crate defines the shell execution environment.
//!
//! A shell execution environment, [`Env`], is a collection of data that may
//! affect or be affected by the execution of commands. The environment consists
//! of application-managed parts and system-managed parts. Application-managed
//! parts are implemented in pure Rust in this crate. Many application-managed
//! parts like [function]s and [variable]s can be manipulated independently of
//! interactions with the underlying system. System-managed parts, on the other
//! hand, depend on the underlying system. Attributes like the working directory
//! and umask are managed by the system to be accessed only by interaction with
//! the system interface.
//!
//! The [`System`] trait is the interface to the system-managed parts.
//! [`RealSystem`] provides an implementation for `System` that interacts with
//! the underlying system. [`VirtualSystem`] is a dummy for simulating the
//! system's behavior without affecting the actual system.

pub mod builtin;
pub mod exec;
pub mod expansion;
pub mod function;
pub mod io;
pub mod job;
mod real_system;
mod system;
pub mod variable;
pub mod virtual_system;

use self::builtin::Builtin;
use self::exec::ExitStatus;
use self::function::FunctionSet;
use self::io::Fd;
use self::job::JobSet;
pub use self::system::SharedSystem;
use self::variable::VariableSet;
use async_trait::async_trait;
use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::sys::select::FdSet;
use nix::sys::signal::SigSet;
use nix::sys::signal::SigmaskHow;
use nix::sys::signal::Signal;
use nix::sys::stat::Mode;
use nix::sys::wait::WaitStatus;
use nix::unistd::Pid;
use std::collections::HashMap;
use std::convert::Infallible;
use std::ffi::CStr;
use std::ffi::CString;
use std::fmt::Debug;
use std::future::ready;
use std::future::Future;
use std::os::raw::c_int;
use std::pin::Pin;
use std::rc::Rc;
use yash_syntax::alias::AliasSet;

/// Whole shell execution environment.
///
/// The shell execution environment consists of application-managed parts and
/// system-managed parts. Application-managed parts are directly implemented in
/// the `Env` instance. System-managed parts are managed by a [`SharedSystem`]
/// that contains an instance of [`System`].
///
/// # Cloning
///
/// `Env::clone` effectively clones the application-managed parts of the
/// environment. Since [`SharedSystem`] is reference-counted, you will not get a
/// deep copy of the system-managed parts. See also
/// [`clone_with_system`](Self::clone_with_system).
#[derive(Clone, Debug)]
pub struct Env {
    /// Aliases defined in the environment.
    ///
    /// The `AliasSet` is reference-counted so that the shell can execute traps
    /// while the parser is reading a command line.
    pub aliases: Rc<AliasSet>,

    /// Built-in utilities available in the environment.
    pub builtins: HashMap<&'static str, Builtin>,

    /// Exit status of the last executed command.
    pub exit_status: ExitStatus,

    /// Functions defined in the environment.
    pub functions: FunctionSet,

    /// Jobs managed in the environment.
    pub jobs: JobSet,

    /// Variables and positional parameters defined in the environment.
    pub variables: VariableSet,

    /// Interface to the system-managed parts of the environment.
    pub system: SharedSystem,
}

/// API to the system-managed parts of the environment.
///
/// The `System` trait defines a collection of methods to access the underlying
/// operating system from the shell as an application program. There are two
/// substantial implementors for this trait: [`RealSystem`] and
/// [`VirtualSystem`]. Another implementor is [`SharedSystem`], which wraps a
/// `System` instance to extend the interface with asynchronous methods.
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
    ///
    /// # Safety
    ///
    /// This function can never be safely called in a multi-threaded process.
    /// [POSIX](https://pubs.opengroup.org/onlinepubs/9699919799/functions/fork.html#tag_16_156_03)
    /// says:
    ///
    /// > If a multi-threaded process calls fork(), the new process shall
    /// contain a replica of the calling thread and its entire address space,
    /// possibly including the states of mutexes and other resources.
    /// Consequently, to avoid errors, the child process may only execute
    /// async-signal-safe operations until such time as one of the exec
    /// functions is called.
    ///
    /// Since this function needs to allocate memory for the returned `Box`,
    /// which is not async-signal-safe, this function must be called in a
    /// single-threaded program.
    unsafe fn new_child_process(&mut self) -> nix::Result<Box<dyn ChildProcess>>;

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

    /// Reports updated status of a child process.
    ///
    /// This is a thin wrapper around the `waitpid` system call. Users of `Env`
    /// should not call it directly. Use dedicated job-managing functions
    /// instead.
    ///
    /// This function is a temporary API that performs synchronous wait by
    /// blocking in the function or by returning a future you need to await.
    /// Eventually, a new function that only polls the state of children will
    /// substitute this function.
    #[deprecated]
    fn wait_sync(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = nix::Result<nix::sys::wait::WaitStatus>>>>;

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

pub use real_system::RealSystem;

pub use virtual_system::VirtualSystem;

impl Env {
    /// Creates a new environment with the given system.
    pub fn with_system(system: Box<dyn System>) -> Env {
        Env {
            aliases: Default::default(),
            builtins: Default::default(),
            exit_status: Default::default(),
            functions: Default::default(),
            jobs: Default::default(),
            variables: Default::default(),
            system: SharedSystem::new(system),
        }
    }

    /// Creates a new environment with a default-constructed [`VirtualSystem`].
    pub fn new_virtual() -> Env {
        Env::with_system(Box::new(VirtualSystem::default()))
    }

    /// Clones this environment.
    ///
    /// The application-managed parts of the environment are cloned normally.
    /// The system-managed parts are replaced with the provided `System`
    /// instance.
    pub fn clone_with_system(&self, system: Box<dyn System>) -> Env {
        Env {
            aliases: self.aliases.clone(),
            builtins: self.builtins.clone(),
            exit_status: self.exit_status,
            functions: self.functions.clone(),
            jobs: self.jobs.clone(),
            variables: self.variables.clone(),
            system: SharedSystem::new(system),
        }
    }

    /// Convenience function that prints the given error message.
    ///
    /// This function prints the `message` to the standard error of this
    /// environment. (The exact format of the printed message is subject to
    /// change.)
    ///
    /// Any errors that may happen writing to the standard error are ignored.
    pub async fn print_error(&mut self, message: &std::fmt::Arguments<'_>) {
        // TODO print `$0` first
        // TODO localize message
        let message = format!("{}\n", message);
        let _: Result<_, _> = self.system.write_all(Fd::STDERR, message.as_bytes()).await;
    }

    /// Convenience function that prints an error message for the given `errno`.
    ///
    /// This function prints `format!("{}: {}\n", message, errno.desc())` to the
    /// standard error of this environment. (The exact format of the printed
    /// message is subject to change.)
    ///
    /// Any errors that may happen writing to the standard error are ignored.
    pub async fn print_system_error(&mut self, errno: Errno, message: &std::fmt::Arguments<'_>) {
        self.print_error(&format_args!("{}: {}", message, errno.desc()))
            .await
    }

    /// Starts a subshell.
    ///
    /// This function creates a new child process in which the argument function
    /// is run. If the child was started successfully, this function returns the
    /// child's process ID. Otherwise, it returns an error.
    ///
    /// Although this function is `async`, it does not wait for the child to
    /// finish, which means the parent and child processes will run
    /// concurrently.
    pub async fn start_subshell<F>(&mut self, f: F) -> nix::Result<Pid>
    where
        F: for<'a> FnOnce(&'a mut Env) -> Pin<Box<dyn Future<Output = ()> + 'a>> + 'static,
    {
        let mut f = Some(f);
        let task: ChildProcessTask = Box::new(move |env| {
            if let Some(f) = f.take() {
                Box::pin(f(env))
            } else {
                Box::pin(ready(()))
            }
        });
        let mut child = unsafe { self.system.new_child_process()? };
        let child_pid = child.run(self, task).await;
        Ok(child_pid)
    }

    /// Runs the argument function in a subshell.
    ///
    /// This function creates a new (real or virtual) subshell in which the
    /// argument function is run and waits for the subshell to finish.
    ///
    /// A real subshell is a subshell that is implemented as a child process of
    /// the current shell process. After executing the argument function, the
    /// child process must immediately exit with the exit status returned from
    /// the function.
    ///
    /// A virtual subshell is a separate shell execution environment that is
    /// simulated in the current shell process. It is used to avoid the overhead
    /// of forking a real subshell when the function's task does not depend on a
    /// real subshell. Note that this function can start with a virtual subshell
    /// and then switch to a real subshell if needed in the middle of the
    /// execution.
    ///
    /// This function does not support job control. If the subshell suspends,
    /// the current shell continues waiting for the subshell to finish, so it
    /// must be resumed by some other means.
    ///
    /// # Return value
    ///
    /// This function usually returns the exit status of the subshell that is
    /// obtained from the subshell environment after the argument function
    /// returns. If an error occurs in creating or awaiting a
    /// [`new_child_process`](System::new_child_process), the error is returned.
    pub async fn run_in_subshell<F>(&mut self, f: F) -> nix::Result<ExitStatus>
    where
        F: for<'a> FnOnce(&'a mut Env) -> Pin<Box<dyn Future<Output = ()> + 'a>> + 'static,
    {
        // TODO Use a virtual subshell when possible
        let child_pid = self.start_subshell(f).await?;

        use nix::sys::wait::WaitStatus::*;
        #[allow(deprecated)]
        match self.system.wait_sync().await? {
            Exited(pid, exit_status) => {
                // TODO This assertion is not correct. We need to handle
                // other possibly existing child processes.
                assert_eq!(pid, child_pid);
                Ok(ExitStatus(exit_status))
            }
            Signaled(pid, signal, _core_dumped) => {
                // TODO This assertion is not correct. We need to handle
                // other possibly existing child processes.
                assert_eq!(pid, child_pid);
                Ok(ExitStatus::from(signal))
            }
            _ => todo!(),
        }
    }

    // TODO Allow specifying which subshell to wait for
    /// Waits for a subshell to terminate, suspend, or resume.
    ///
    /// This function waits for a subshell to change its execution status.
    /// If the current environment has no subshell, it returns
    /// `Err(Errno::ECHILD)`.
    pub async fn wait_for_subshell(&mut self) -> nix::Result<WaitStatus> {
        // We need to set the signal handling before calling `wait` so we don't
        // miss any `SIGCHLD` that may arrive between `wait` and
        // `wait_for_signal`.
        self.system
            .set_signal_handling(Signal::SIGCHLD, SignalHandling::Catch)?;

        loop {
            match self.system.wait()? {
                WaitStatus::StillAlive => {}
                new_status => return Ok(new_status),
            }
            self.system.wait_for_signal(Signal::SIGCHLD).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virtual_system::SystemState;
    use futures_executor::block_on;
    use futures_executor::LocalPool;
    use futures_util::task::LocalSpawnExt;

    #[test]
    fn print_system_error_einval() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        block_on(env.print_system_error(Errno::EINVAL, &format_args!("dummy message {}", 42)));

        let state = state.borrow();
        let stderr = state.file_system.get("/dev/stderr").unwrap().borrow();
        let message = format!("dummy message {}: {}\n", 42, Errno::EINVAL.desc());
        assert_eq!(stderr.content, message.as_bytes());
    }

    #[test]
    fn run_in_subshell_with_child_normally_exiting() {
        let system = VirtualSystem::new();
        let mut executor = LocalPool::new();
        system.state.borrow_mut().executor = Some(Rc::new(executor.spawner()));

        let status = ExitStatus(97);
        let mut env = Env::with_system(Box::new(system));
        let result = executor.run_until(env.run_in_subshell(move |env| {
            Box::pin(async move {
                env.exit_status = status;
            })
        }));
        assert_eq!(result, Ok(status));
    }

    // TODO Test case for parent with signaled child

    #[test]
    fn run_in_subshell_with_fork_failure() {
        let system = VirtualSystem::new();
        let mut executor = LocalPool::new();
        let mut env = Env::with_system(Box::new(system));
        let result = executor.run_until(env.run_in_subshell(|_env| {
            Box::pin(async {
                panic!("Not expected to reach here");
            })
        }));
        assert_eq!(result, Err(Errno::ENOSYS));
    }

    #[test]
    fn start_and_wait_for_subshell() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut executor = LocalPool::new();
        state.borrow_mut().executor = Some(Rc::new(executor.spawner()));

        let mut env = Env::with_system(Box::new(system));
        let shared_system = env.system.clone();
        let task = executor
            .spawner()
            .spawn_local_with_handle(async move {
                let pid = env
                    .start_subshell(|env| {
                        Box::pin(async move {
                            env.exit_status = ExitStatus(42);
                        })
                    })
                    .await
                    .unwrap();
                let result = env.wait_for_subshell().await;
                assert_eq!(result, Ok(WaitStatus::Exited(pid, 42)));
            })
            .unwrap();
        executor.run_until_stalled();
        shared_system.select().unwrap();
        SystemState::select_all(&state);
        executor.run_until(task);
    }

    #[test]
    fn wait_for_subshell_no_subshell() {
        let system = VirtualSystem::new();
        let mut executor = LocalPool::new();
        system.state.borrow_mut().executor = Some(Rc::new(executor.spawner()));
        let mut env = Env::with_system(Box::new(system));
        executor.run_until(async move {
            let result = env.wait_for_subshell().await;
            assert_eq!(result, Err(Errno::ECHILD));
        });
    }
}
