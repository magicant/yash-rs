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
pub mod function;
pub mod input;
pub mod io;
pub mod job;
pub mod option;
pub mod semantics;
pub mod stack;
pub mod system;
pub mod trap;
pub mod variable;

use self::builtin::Builtin;
use self::function::FunctionSet;
use self::io::Fd;
use self::job::JobSet;
use self::job::Pid;
use self::job::WaitStatus;
use self::job::WaitStatusEx;
use self::option::AllExport;
use self::option::OptionSet;
use self::option::{Off, On};
use self::semantics::ExitStatus;
use self::stack::Frame;
use self::stack::Stack;
pub use self::system::r#virtual::VirtualSystem;
pub use self::system::real::RealSystem;
use self::system::ChildProcessTask;
pub use self::system::SharedSystem;
use self::system::SignalHandling;
pub use self::system::System;
use self::trap::Signal;
use self::trap::TrapSet;
use self::variable::ReadOnlyError;
use self::variable::Scope;
use self::variable::Variable;
use self::variable::VariableSet;
use async_trait::async_trait;
use futures_util::task::noop_waker_ref;
use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::ops::ControlFlow::{Break, Continue};
use std::pin::Pin;
use std::rc::Rc;
use std::task::Context;
use std::task::Poll;
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
#[non_exhaustive]
pub struct Env {
    /// Aliases defined in the environment.
    pub aliases: AliasSet,

    /// Name of the current shell executable or shell script.
    ///
    /// Special parameter `0` expands to this value.
    pub arg0: String,

    /// Built-in utilities available in the environment.
    pub builtins: HashMap<&'static str, Builtin>,

    /// Exit status of the last executed command.
    pub exit_status: ExitStatus,

    /// Functions defined in the environment.
    pub functions: FunctionSet,

    /// Jobs managed in the environment.
    pub jobs: JobSet,

    /// Process ID of the main shell process.
    ///
    /// This PID represents the value of the `$` special parameter.
    pub main_pid: Pid,

    /// Shell option settings.
    pub options: OptionSet,

    /// Runtime execution context stack.
    pub stack: Stack,

    /// Traps defined in the environment.
    pub traps: TrapSet,

    /// Variables and positional parameters defined in the environment.
    pub variables: VariableSet,

    /// Interface to the system-managed parts of the environment.
    pub system: SharedSystem,
}

impl Env {
    /// Creates a new environment with the given system.
    ///
    /// Members of the new environments are default-constructed except that:
    /// - `main_pid` is initialized as `system.getpid()`
    /// - `system` is initialized as `SharedSystem::new(system)`
    #[must_use]
    pub fn with_system(system: Box<dyn System>) -> Env {
        Env {
            aliases: Default::default(),
            arg0: Default::default(),
            builtins: Default::default(),
            exit_status: Default::default(),
            functions: Default::default(),
            jobs: Default::default(),
            main_pid: system.getpid(),
            options: Default::default(),
            stack: Default::default(),
            traps: Default::default(),
            variables: Default::default(),
            system: SharedSystem::new(system),
        }
    }

    /// Creates a new environment with a default-constructed [`VirtualSystem`].
    #[must_use]
    pub fn new_virtual() -> Env {
        Env::with_system(Box::new(VirtualSystem::default()))
    }

    /// Clones this environment.
    ///
    /// The application-managed parts of the environment are cloned normally.
    /// The system-managed parts are replaced with the provided `System`
    /// instance.
    #[must_use]
    pub fn clone_with_system(&self, system: Box<dyn System>) -> Env {
        Env {
            aliases: self.aliases.clone(),
            arg0: self.arg0.clone(),
            builtins: self.builtins.clone(),
            exit_status: self.exit_status,
            functions: self.functions.clone(),
            jobs: self.jobs.clone(),
            main_pid: self.main_pid,
            options: self.options,
            stack: self.stack.clone(),
            traps: self.traps.clone(),
            variables: self.variables.clone(),
            system: SharedSystem::new(system),
        }
    }

    /// Convenience function that prints the given error message.
    ///
    /// This function prints the `message` to the standard error of this
    /// environment, ignoring any errors that may happen.
    ///
    /// No formatting takes place in this function. Use [`io::print_message`] or
    /// [`io::print_error`] to format a message before printing.
    pub async fn print_error(&mut self, message: &str) {
        let _: Result<_, _> = self.system.write_all(Fd::STDERR, message.as_bytes()).await;
    }

    /// Waits for some signals to be caught in the current process.
    ///
    /// Returns an array of signals caught.
    ///
    /// This function is a wrapper for [`SharedSystem::wait_for_signals`].
    /// Before the function returns, it passes the results to
    /// [`TrapSet::catch_signal`] so the trap set can remember the signals
    /// caught to be handled later.
    pub async fn wait_for_signals(&mut self) -> Rc<[Signal]> {
        let result = self.system.wait_for_signals().await;
        for signal in result.iter().copied() {
            self.traps.catch_signal(signal);
        }
        result
    }

    /// Waits for a specific signal to be caught in the current process.
    ///
    /// This function calls [`wait_for_signals`](Self::wait_for_signals)
    /// repeatedly until it returns results containing the specified `signal`.
    pub async fn wait_for_signal(&mut self, signal: Signal) {
        while !self.wait_for_signals().await.contains(&signal) {}
    }

    /// Returns signals that have been caught.
    ///
    /// This function is similar to
    /// [`wait_for_signals`](Self::wait_for_signals) but does not wait for
    /// signals to be caught. Instead, it only checks if any signals have been
    /// caught but not yet consumed in the [`SharedSystem`].
    pub fn poll_signals(&mut self) -> Option<Rc<[Signal]>> {
        let system = self.system.clone();

        let future = self.wait_for_signals();
        futures_util::pin_mut!(future);

        let mut context = Context::from_waker(noop_waker_ref());
        if let Poll::Ready(signals) = future.as_mut().poll(&mut context) {
            return Some(signals);
        }

        system.select(true).ok();
        if let Poll::Ready(signals) = future.poll(&mut context) {
            return Some(signals);
        }
        None
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
    ///
    /// If function `f` returns an `Err(Divert::...)`, it is handled as follows:
    ///
    /// - `Interrupt` and `Exit` with `Some(exit_status)` override the exit
    ///   status in `Env`.
    /// - Other `Divert` values are ignored.
    ///
    /// This function does not add the created subshell to `self.jobs`. You have
    /// to do it for yourself.
    pub async fn start_subshell<F>(&mut self, f: F) -> nix::Result<Pid>
    where
        F: for<'a> FnOnce(
                &'a mut Env,
            )
                -> Pin<Box<dyn Future<Output = self::semantics::Result> + 'a>>
            + 'static,
        // TODO Revisit to simplify this function type when GAT is stabilized
    {
        let mut f = Some(f);
        let task: ChildProcessTask = Box::new(move |env| {
            let f = f.take().expect("child process task should run only once");
            Box::pin(async move {
                let mut env = env.push_frame(Frame::Subshell);
                let env = &mut *env;
                env.traps.enter_subshell(&mut env.system);
                match f(env).await {
                    Continue(()) => (),
                    Break(divert) => {
                        if let Some(exit_status) = divert.exit_status() {
                            env.exit_status = exit_status
                        }
                    }
                }
            })
        });
        let mut child = self.system.new_child_process()?;
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
    /// See [`start_subshell`](Self::start_subshell) for the behavior of the
    /// subshell in case function `f` returns an `Err(Divert::...)`.
    ///
    /// # Return value
    ///
    /// This function usually returns the exit status of the subshell that is
    /// obtained from the subshell environment after the argument function
    /// returns. If an error occurs in creating or awaiting a
    /// [`new_child_process`](System::new_child_process), the error is returned.
    pub async fn run_in_subshell<F>(&mut self, f: F) -> nix::Result<ExitStatus>
    where
        F: for<'a> FnOnce(
                &'a mut Env,
            )
                -> Pin<Box<dyn Future<Output = self::semantics::Result> + 'a>>
            + 'static,
    {
        // TODO Use a virtual subshell when possible
        let child_pid = self.start_subshell(f).await?;

        let (awaited_pid, exit_status) = self.wait_for_subshell_to_finish(child_pid).await?;
        assert_eq!(awaited_pid, child_pid);
        Ok(exit_status)
    }

    /// Waits for a subshell to terminate, suspend, or resume.
    ///
    /// This function waits for a subshell to change its execution status. The
    /// `target` parameter specifies which child to wait for:
    ///
    /// - `-1`: any child
    /// - `0`: any child in the same process group as the current process
    /// - `pid`: the child whose process ID is `pid`
    /// - `-pgid`: any child in the process group whose process group ID is `pgid`
    ///
    /// When [`self.system.wait`](System::wait) returned a new status of the
    /// target, it is sent to `self.jobs` ([`JobSet::update_status`]) before
    /// being returned from this function.
    ///
    /// If there is no matching target, this function returns
    /// `Err(Errno::ECHILD)`.
    pub async fn wait_for_subshell(&mut self, target: Pid) -> nix::Result<WaitStatus> {
        // We need to set the signal handling before calling `wait` so we don't
        // miss any `SIGCHLD` that may arrive between `wait` and `wait_for_signal`.
        self.traps.enable_sigchld_handler(&mut self.system)?;

        loop {
            match self.system.wait(target) {
                Ok(WaitStatus::StillAlive) => {}
                Ok(status) => {
                    self.jobs.update_status(status);
                    return Ok(status);
                }
                result => return result,
            }
            self.wait_for_signal(Signal::SIGCHLD).await;
        }
    }

    /// Wait for a subshell to terminate.
    ///
    /// This function is similar to
    /// [`wait_for_subshell`](Self::wait_for_subshell), but returns only when
    /// the target is finished (either exited or killed by a signal).
    ///
    /// Returns the process ID of the awaited process and its exit status.
    pub async fn wait_for_subshell_to_finish(
        &mut self,
        target: Pid,
    ) -> nix::Result<(Pid, ExitStatus)> {
        loop {
            let wait_status = self.wait_for_subshell(target).await?;
            if wait_status.is_finished() {
                return Ok((wait_status.pid().unwrap(), wait_status.try_into().unwrap()));
            }
        }
    }

    /// Applies all job status updates to jobs in `self.jobs`.
    ///
    /// This function calls [`self.system.wait`](System::wait) repeatedly until
    /// all status updates available are applied to `self.jobs`
    /// ([`JobSet::update_status`]).
    ///
    /// Note that updates of subshells that are not managed in `self.jobs` are
    /// lost when you call this function.
    pub fn update_all_subshell_statuses(&mut self) {
        loop {
            match self.system.wait(Pid::from_raw(-1)) {
                Ok(WaitStatus::StillAlive) => break,
                Ok(status) => self.jobs.update_status(status),
                Err(_) => break,
            };
        }
    }

    /// Assigns a variable.
    ///
    /// This function is a thin wrapper around [`VariableSet::assign`] that
    /// automatically applies the `AllExport` [shell
    /// option](crate::option::Option). You should always prefer this unless you
    /// want to ignore the option.
    pub fn assign_variable(
        &mut self,
        scope: Scope,
        name: String,
        value: Variable,
    ) -> Result<Option<Variable>, ReadOnlyError> {
        let value = match self.options.get(AllExport) {
            On => value.export(),
            Off => value,
        };
        self.variables.assign(scope, name, value)
    }
}

#[async_trait(?Send)]
impl io::Stderr for Env {
    async fn print_error(&mut self, message: &str) {
        self.print_error(message).await
    }
    fn should_print_error_in_color(&self) -> bool {
        // TODO Enable color depending on user config (force/auto/never)
        // TODO Check if the terminal really supports color (needs terminfo)
        self.system.isatty(Fd::STDERR) == Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::Job;
    use crate::system::r#virtual::SystemState;
    use crate::system::Errno;
    use crate::trap::Trap;
    use assert_matches::assert_matches;
    use futures_executor::LocalPool;
    use futures_util::task::LocalSpawnExt;
    use std::cell::Cell;
    use std::cell::RefCell;
    use std::ops::ControlFlow::Continue;
    use yash_syntax::source::Location;

    /// Helper function to perform a test in a virtual system with an executor.
    pub fn in_virtual_system<F, Fut>(f: F)
    where
        F: FnOnce(Env, Pid, Rc<RefCell<SystemState>>) -> Fut,
        Fut: Future<Output = ()> + 'static,
    {
        let system = VirtualSystem::new();
        let pid = system.process_id;
        let state = Rc::clone(&system.state);
        let mut executor = futures_executor::LocalPool::new();
        state.borrow_mut().executor = Some(Rc::new(executor.spawner()));

        let env = Env::with_system(Box::new(system));
        let shared_system = env.system.clone();
        let task = f(env, pid, Rc::clone(&state));
        let done = Rc::new(Cell::new(false));
        let done_2 = Rc::clone(&done);

        executor
            .spawner()
            .spawn_local(async move {
                task.await;
                done.set(true);
            })
            .unwrap();

        while !done_2.get() {
            executor.run_until_stalled();
            shared_system.select(false).unwrap();
            SystemState::select_all(&state);
        }
    }

    #[test]
    fn wait_for_signal_remembers_signal_in_trap_set() {
        in_virtual_system(|mut env, pid, state| async move {
            env.traps
                .set_trap(
                    &mut env.system,
                    Signal::SIGCHLD,
                    Trap::Command("".into()),
                    Location::dummy(""),
                    false,
                )
                .unwrap();
            {
                let mut state = state.borrow_mut();
                let process = state.processes.get_mut(&pid).unwrap();
                assert!(process.blocked_signals().contains(Signal::SIGCHLD));
                process.raise_signal(Signal::SIGCHLD);
            }
            env.wait_for_signal(Signal::SIGCHLD).await;

            let trap_state = env.traps.get_trap(Signal::SIGCHLD).0.unwrap();
            assert!(trap_state.pending);
        })
    }

    fn poll_signals_env() -> (Env, VirtualSystem) {
        let system = VirtualSystem::new();
        let shared_system = SharedSystem::new(Box::new(system.clone()));
        let mut env = Env::with_system(Box::new(shared_system));
        env.traps
            .set_trap(
                &mut env.system,
                Signal::SIGCHLD,
                Trap::Command("".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        (env, system)
    }

    #[test]
    fn poll_signals_none() {
        let mut env = poll_signals_env().0;
        let result = env.poll_signals();
        assert_eq!(result, None);
    }

    #[test]
    fn poll_signals_some() {
        let (mut env, system) = poll_signals_env();
        {
            let mut state = system.state.borrow_mut();
            let process = state.processes.get_mut(&system.process_id).unwrap();
            assert!(process.blocked_signals().contains(Signal::SIGCHLD));
            process.raise_signal(Signal::SIGCHLD);
        }

        let result = env.poll_signals().unwrap();
        assert_eq!(*result, [Signal::SIGCHLD]);
    }

    #[test]
    fn run_in_subshell_with_child_normally_exiting() {
        in_virtual_system(|mut env, _pid, _state| async move {
            let status = ExitStatus(97);
            let result = env
                .run_in_subshell(move |env| {
                    Box::pin(async move {
                        env.exit_status = status;
                        Continue(())
                    })
                })
                .await;
            assert_eq!(result, Ok(status));
        });
    }

    // TODO Test case for parent with signaled child
    // TODO Test case for parent with stopped child

    #[test]
    fn run_in_subshell_with_fork_failure() {
        let system = VirtualSystem::new();
        let mut executor = LocalPool::new();
        let mut env = Env::with_system(Box::new(system));
        let result = executor
            .run_until(env.run_in_subshell(|_env| unreachable!("subshell not expected to run")));
        assert_eq!(result, Err(Errno::ENOSYS));
    }

    #[test]
    fn start_and_wait_for_subshell() {
        in_virtual_system(|mut env, _pid, _state| async move {
            let pid = env
                .start_subshell(|env| {
                    Box::pin(async move {
                        env.exit_status = ExitStatus(42);
                        Continue(())
                    })
                })
                .await
                .unwrap();
            let result = env.wait_for_subshell(pid).await;
            assert_eq!(result, Ok(WaitStatus::Exited(pid, 42)));
        });
    }

    #[test]
    fn start_and_wait_for_subshell_with_job_set() {
        in_virtual_system(|mut env, _pid, _state| async move {
            let pid = env
                .start_subshell(|env| {
                    Box::pin(async move {
                        env.exit_status = ExitStatus(42);
                        Continue(())
                    })
                })
                .await
                .unwrap();
            let mut job = Job::new(pid);
            job.name = "my job".to_string();
            let job_index = env.jobs.add(job.clone());
            let result = env.wait_for_subshell(pid).await;
            assert_eq!(result, Ok(WaitStatus::Exited(pid, 42)));
            job.status = WaitStatus::Exited(pid, 42);
            assert_eq!(env.jobs.get(job_index), Some(&job));
        });
    }

    #[test]
    fn stack_frame_in_subshell() {
        in_virtual_system(|mut env, _pid, _state| async move {
            env.run_in_subshell(|env| {
                Box::pin(async move {
                    assert_eq!(env.stack[..], [Frame::Subshell]);
                    Continue(())
                })
            })
            .await
            .unwrap();
            assert_eq!(env.stack[..], []);
        });
    }

    #[test]
    fn trap_reset_in_subshell() {
        in_virtual_system(|mut env, _pid, _state| async move {
            env.traps
                .set_trap(
                    &mut env.system,
                    Signal::SIGCHLD,
                    Trap::Command("echo foo".into()),
                    Location::dummy(""),
                    false,
                )
                .unwrap();
            let pid = env
                .start_subshell(|env| {
                    Box::pin(async move {
                        let trap_state = assert_matches!(
                            env.traps.get_trap(Signal::SIGCHLD),
                            (None, Some(trap_state)) => trap_state
                        );
                        assert_matches!(
                            &trap_state.action,
                            Trap::Command(body) => assert_eq!(body, "echo foo")
                        );
                        Continue(())
                    })
                })
                .await
                .unwrap();
            env.wait_for_subshell(pid).await.unwrap();
        });
    }

    #[test]
    fn wait_for_subshell_no_subshell() {
        let system = VirtualSystem::new();
        let mut executor = LocalPool::new();
        system.state.borrow_mut().executor = Some(Rc::new(executor.spawner()));
        let mut env = Env::with_system(Box::new(system));
        executor.run_until(async move {
            let result = env.wait_for_subshell(Pid::from_raw(-1)).await;
            assert_eq!(result, Err(Errno::ECHILD));
        });
    }

    #[test]
    fn update_all_subshell_statuses_without_subshells() {
        let mut env = Env::new_virtual();
        env.update_all_subshell_statuses();
    }

    #[test]
    fn update_all_subshell_statuses_with_subshells() {
        let system = VirtualSystem::new();
        let mut executor = futures_executor::LocalPool::new();
        system.state.borrow_mut().executor = Some(Rc::new(executor.spawner()));

        let mut env = Env::with_system(Box::new(system));

        let [(pid_1, job_1), (pid_2, job_2), (_pid_3, job_3)] = executor.run_until(async {
            // Run a subshell.
            let pid_1 = env
                .start_subshell(|env| {
                    Box::pin(async move {
                        env.exit_status = ExitStatus(12);
                        Continue(())
                    })
                })
                .await
                .unwrap();
            // Run another subshell.
            let pid_2 = env
                .start_subshell(|env| {
                    Box::pin(async move {
                        env.exit_status = ExitStatus(35);
                        Continue(())
                    })
                })
                .await
                .unwrap();
            // This one will never finish.
            let pid_3 = env
                .start_subshell(|_env| Box::pin(futures_util::future::pending()))
                .await
                .unwrap();
            // Yet another subshell. We don't make this into a job.
            let _ = env
                .start_subshell(|env| {
                    Box::pin(async move {
                        env.exit_status = ExitStatus(100);
                        Continue(())
                    })
                })
                .await
                .unwrap();
            // Create jobs.
            let job_1 = env.jobs.add(Job::new(pid_1));
            let job_2 = env.jobs.add(Job::new(pid_2));
            let job_3 = env.jobs.add(Job::new(pid_3));
            [(pid_1, job_1), (pid_2, job_2), (pid_3, job_3)]
        });

        // Let the jobs (except job_3) finish.
        executor.run_until_stalled();

        // We're not yet updated.
        assert_eq!(env.jobs.get(job_1).unwrap().status, WaitStatus::StillAlive);
        assert_eq!(env.jobs.get(job_2).unwrap().status, WaitStatus::StillAlive);
        assert_eq!(env.jobs.get(job_3).unwrap().status, WaitStatus::StillAlive);

        env.update_all_subshell_statuses();

        // Now we have the results.
        assert_eq!(
            env.jobs.get(job_1).unwrap().status,
            WaitStatus::Exited(pid_1, 12)
        );
        assert_eq!(
            env.jobs.get(job_2).unwrap().status,
            WaitStatus::Exited(pid_2, 35)
        );
        assert_eq!(env.jobs.get(job_3).unwrap().status, WaitStatus::StillAlive);
    }

    #[test]
    fn assign_variable_with_all_export_off() {
        let mut env = Env::new_virtual();
        let a = Variable::new("A");
        let result = env.assign_variable(Scope::Global, "a".to_string(), a.clone());
        assert_eq!(result, Ok(None));
        let b = Variable::new("B").export();
        let result = env.assign_variable(Scope::Global, "b".to_string(), b.clone());
        assert_eq!(result, Ok(None));

        assert_eq!(env.variables.get("a").unwrap(), &a);
        assert_eq!(env.variables.get("b").unwrap(), &b);
    }

    #[test]
    fn assign_variable_with_all_export_on() {
        let mut env = Env::new_virtual();
        env.options.set(AllExport, On);
        let a = Variable::new("A");
        let result = env.assign_variable(Scope::Global, "a".to_string(), a.clone());
        assert_eq!(result, Ok(None));
        let b = Variable::new("B").export();
        let result = env.assign_variable(Scope::Global, "b".to_string(), b.clone());
        assert_eq!(result, Ok(None));

        assert_eq!(env.variables.get("a").unwrap(), &a.export());
        assert_eq!(env.variables.get("b").unwrap(), &b);
    }
}
