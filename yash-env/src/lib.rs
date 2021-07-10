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
//! affect or be affected by execution of commands. The environment consists of
//! application-managed parts and system-managed parts. Application-managed
//! parts are implemented in pure Rust in this crate. Many application-managed
//! parts like [function]s and [variable]s can be manipulated independently of
//! interactions with the underlying system. System-managed parts, on the other
//! hand, depend on the underlying system. Attributes like the working directory
//! and umask are managed by the system, so they can be accessed only by
//! interaction with the system interface.
//!
//! The system-managed parts are abstracted as the [`System`] trait.
//! [`RealSystem`] provides an implementation for `System` that interacts with
//! the underlying system. [`VirtualSystem`] is a dummy for simulation that
//! works without affecting the actual system.

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
use crate::exec::Divert;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::ffi::CStr;
use std::ffi::CString;
use std::fmt::Debug;
use std::rc::Rc;
use yash_syntax::alias::AliasSet;

/// Whole shell execution environment.
///
/// The shell execution environment consists of application-managed parts and
/// system-managed parts. Application-managed parts are directly implemented in
/// the `Env` instance. System-managed parts are abstracted as [`System`] so
/// that you can replace them with a dummy implementation.
///
/// The `Clone` implementation for `Env` duplicates application-managed parts.
/// The system-managed parts are shared among cloned `Env`s as the `System`
/// instance is contained in `Rc<RefCell<_>>`.
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
    ///
    /// Since the `System` instance is contained in `Rc<RefCell<_>>`, you need
    /// to borrow the `RefCell` dynamically to access the `System`. Note that if
    /// you `.await` a future while keeping the borrow, the current thread may
    /// switch to another async task that may simultaneously borrow the
    /// `System`, resulting in a panic! Make sure you drop the borrow (`Ref` or
    /// `RefMut`) before `.await`ing.
    ///
    /// TODO Add example code illustrating the borrow-and-await problem.
    pub system: Rc<RefCell<dyn System>>,
}

/// Abstraction of the system-managed parts of the environment.
///
/// TODO Elaborate
pub trait System: Debug {
    /// Whether there is an executable file at the specified path.
    fn is_executable_file(&self, path: &CStr) -> bool;

    /// Clones the current shell process.
    ///
    /// This is a thin wrapper around the `fork` system call. Users of `Env`
    /// should not call it directly. Instead, use [`Env::run_in_subshell`] so
    /// that the environment can manage the created child process as a job
    /// member.
    ///
    /// # Safety
    ///
    /// See [nix's documentation](nix::unistd::fork) to learn why this function
    /// is unsafe.
    unsafe fn fork(&mut self) -> nix::Result<nix::unistd::ForkResult>;

    /// Reports updated status of a child process.
    ///
    /// This is a thin wrapper around the `waitpid` system call. Users of `Env`
    /// should not call it directly. Use dedicated job-managing functions
    /// instead.
    ///
    /// TODO Describe the non-blocking nature of this function
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

pub use real_system::RealSystem;

pub use virtual_system::VirtualSystem;

impl Env {
    /// Creates a new environment with the given system.
    pub fn with_system(system: Rc<RefCell<dyn System>>) -> Env {
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
        Env::with_system(Rc::new(RefCell::new(VirtualSystem::default())))
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
    /// If this function chooses to create a real subshell, it returns twice:
    /// once in the current (original) shell process and the other in the child
    /// process.
    ///
    /// In the current process, this function usually returns `Ok(Ok(e))`, where
    /// `e` is the exit status of the subshell that is obtained from the
    /// subshell environment after the argument function returns. If an error
    /// occurs in [`fork`](System::fork), the error is returned as
    /// `Ok(Err(...))`.
    ///
    /// In the child process, this function returns `Err(Divert::Exit(e))`. The
    /// `Divert` value must be propagated in the call stack so that the child
    /// process immediately exits with the exit status contained in the value.
    pub fn run_in_subshell<F>(&mut self, f: F) -> exec::Result<nix::Result<ExitStatus>>
    where
        F: FnOnce(&mut Env),
    {
        // TODO Use a virtual subshell when possible
        use nix::sys::wait::WaitStatus;
        use nix::unistd::ForkResult::*;
        let mut system = self.system.borrow_mut();
        match unsafe { system.fork() } {
            Ok(Parent { child }) => {
                let wait_result = system.wait();
                drop(system);
                match wait_result {
                    Ok(WaitStatus::Exited(pid, exit_status)) => {
                        // TODO This assertion is not correct. We need to handle
                        // other possibly existing child processes.
                        assert_eq!(pid, child);
                        Ok(Ok(ExitStatus(exit_status)))
                    }
                    Ok(WaitStatus::Signaled(pid, _signal, _core_dumped)) => {
                        // TODO This assertion is not correct. We need to handle
                        // other possibly existing child processes.
                        assert_eq!(pid, child);
                        // TODO Convert signal to exit status
                        Ok(Ok(ExitStatus(128)))
                    }
                    Ok(_) => todo!(),
                    Err(e) => Ok(Err(e)),
                }
            }
            Ok(Child) => {
                drop(system);
                // TODO Reset traps
                // TODO Push a subshell state to the execution context stack
                f(self);
                Err(Divert::Exit(self.exit_status))
                // TODO Should we directly exit here rather than returning
                // Divert::Exit?
                //  - Does stack unwinding not have any unexpected side effect?
                //  - How does the caller know that we're exiting from a
                //    subshell, not from the main shell process? For example,
                //    an interactive shell with a suspended job may refuse to
                //    exit.
            }
            Err(e) => Ok(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::sys::wait::WaitStatus;
    use nix::unistd::ForkResult;
    use nix::unistd::Pid;

    #[test]
    fn run_in_subshell_parent() {
        let child = Pid::from_raw(12345);
        let status = 97;
        let mut system = VirtualSystem::default();
        system
            .pending_forks
            .push_back(Ok(ForkResult::Parent { child }));
        system
            .pending_waits
            .push_back(Ok(WaitStatus::Exited(child, status)));
        let mut env = Env::with_system(Rc::new(RefCell::new(system)));
        let result = env.run_in_subshell(|_| unreachable!());
        assert_eq!(result, Ok(Ok(ExitStatus(status))));
    }

    // TODO Test case for parent with signaled child

    #[test]
    fn run_in_subshell_child() {
        let status = ExitStatus(71);
        let mut system = VirtualSystem::default();
        system.pending_forks.push_back(Ok(ForkResult::Child));
        let mut env = Env::with_system(Rc::new(RefCell::new(system)));
        let result = env.run_in_subshell(|env| env.exit_status = status);
        assert_eq!(result, Err(Divert::Exit(status)));
    }

    // TODO Test case where fork fails
}
