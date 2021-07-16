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
pub mod job;
mod real_system;
pub mod variable;
pub mod virtual_system;

use self::builtin::Builtin;
use self::exec::ExitStatus;
use self::function::FunctionSet;
use self::job::JobSet;
use self::variable::VariableSet;
use async_trait::async_trait;
use nix::unistd::Pid;
use std::collections::HashMap;
use std::convert::Infallible;
use std::ffi::CStr;
use std::ffi::CString;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use yash_syntax::alias::AliasSet;

/// Whole shell execution environment.
///
/// The shell execution environment consists of application-managed parts and
/// system-managed parts. Application-managed parts are directly implemented in
/// the `Env` instance. System-managed parts are abstracted as [`System`] so
/// that they can be replaced with a dummy implementation.
#[derive(Debug)]
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
    pub system: Box<dyn System>,
}

/// Abstraction of the system-managed parts of the environment.
///
/// TODO Elaborate
pub trait System: Debug {
    /// Whether there is an executable file at the specified path.
    fn is_executable_file(&self, path: &CStr) -> bool;

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
    /// This is a thin wrapper around the `waitpid` system call. Users of `Env`
    /// should not call it directly. Use dedicated job-managing functions
    /// instead.
    ///
    /// TODO Describe the non-blocking nature of this function
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
    ) -> Pin<Box<dyn Future<Output = nix::Result<nix::sys::wait::WaitStatus>> + '_>>;

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
    async fn run(
        &mut self,
        env: &mut Env,
        task: Box<dyn for<'a> FnMut(&'a mut Env) -> Pin<Box<dyn Future<Output = ()> + 'a>>>,
    ) -> Pid;
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
            system,
        }
    }

    /// Creates a new empty virtual environment.
    pub fn new_virtual() -> Env {
        Env::with_system(Box::new(VirtualSystem::default()))
    }

    /// Clones this environment.
    ///
    /// The application-managed parts of the environment are cloned normally.
    /// The system-managed parts cannot be cloned, so you must provide a
    /// `System` instance.
    pub fn clone_with_system(&self, system: Box<dyn System>) -> Env {
        Env {
            aliases: self.aliases.clone(),
            builtins: self.builtins.clone(),
            exit_status: self.exit_status,
            functions: self.functions.clone(),
            jobs: self.jobs.clone(),
            variables: self.variables.clone(),
            system,
        }
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
        F: FnOnce(&mut Env) + 'static,
    {
        // TODO Use a virtual subshell when possible
        let mut f = Some(f);

        let child_pid = unsafe { self.system.new_child_process()? }
            .run(
                self,
                Box::new(move |env| {
                    if let Some(f) = f.take() {
                        Box::pin(async move { f(env) })
                    } else {
                        Box::pin(async {})
                    }
                }),
            )
            .await;

        use nix::sys::wait::WaitStatus::*;
        #[allow(deprecated)]
        match self.system.wait_sync().await? {
            Exited(pid, exit_status) => {
                // TODO This assertion is not correct. We need to handle
                // other possibly existing child processes.
                assert_eq!(pid, child_pid);
                Ok(ExitStatus(exit_status))
            }
            Signaled(pid, _signal, _core_dumped) => {
                // TODO This assertion is not correct. We need to handle
                // other possibly existing child processes.
                assert_eq!(pid, child_pid);
                // TODO Convert signal to exit status
                Ok(ExitStatus(128))
            }
            _ => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::LocalPool;

    #[test]
    fn run_in_subshell_with_child_normally_exiting() {
        let system = VirtualSystem::new();
        let mut executor = LocalPool::new();
        let mut state = system.state.borrow_mut();
        state.executor = Some(Rc::new(executor.spawner()));
        drop(state);

        let status = ExitStatus(97);
        let mut env = Env::with_system(Box::new(system));
        let result = executor.run_until(env.run_in_subshell(move |env| env.exit_status = status));
        assert_eq!(result, Ok(status));
    }

    // TODO Test case for parent with signaled child

    // TODO Test case where fork fails
}
