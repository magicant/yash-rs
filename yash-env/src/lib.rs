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

use self::builtin::getopts::GetoptsState;
use self::builtin::Builtin;
use self::function::FunctionSet;
use self::io::Fd;
use self::job::JobSet;
use self::job::Pid;
use self::job::WaitStatus;
use self::job::WaitStatusEx;
use self::option::On;
use self::option::OptionSet;
use self::option::{AllExport, ErrExit, Monitor};
use self::semantics::Divert;
use self::semantics::ExitStatus;
use self::stack::Frame;
use self::stack::Stack;
pub use self::system::r#virtual::VirtualSystem;
pub use self::system::real::RealSystem;
pub use self::system::SharedSystem;
use self::system::SignalHandling;
pub use self::system::System;
use self::system::SystemEx;
use self::trap::Signal;
use self::trap::TrapSet;
use self::variable::Scope;
use self::variable::VariableRefMut;
use self::variable::VariableSet;
use futures_util::task::noop_waker_ref;
use std::collections::HashMap;
use std::ffi::CStr;
use std::fmt::Debug;
use std::future::Future;
use std::ops::ControlFlow::{self, Break, Continue};
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
    /// Aliases defined in the environment
    pub aliases: AliasSet,

    /// Name of the current shell executable or shell script
    ///
    /// Special parameter `0` expands to this value.
    pub arg0: String,

    /// Built-in utilities available in the environment
    pub builtins: HashMap<&'static str, Builtin>,

    /// Exit status of the last executed command
    pub exit_status: ExitStatus,

    /// Functions defined in the environment
    pub functions: FunctionSet,

    /// State of the previous invocation of the `getopts` built-in
    pub getopts_state: Option<GetoptsState>,

    /// Jobs managed in the environment
    pub jobs: JobSet,

    /// Process group ID of the main shell process
    pub main_pgid: Pid,

    /// Process ID of the main shell process
    ///
    /// This PID represents the value of the `$` special parameter.
    pub main_pid: Pid,

    /// Shell option settings
    pub options: OptionSet,

    /// Runtime execution context stack
    pub stack: Stack,

    /// Traps defined in the environment
    pub traps: TrapSet,

    /// File descriptor to the controlling terminal
    ///
    /// [`get_tty`](Self::get_tty) saves a file descriptor in this variable, so
    /// you don't have to prepare it yourself.
    pub tty: Option<Fd>,

    /// Variables and positional parameters defined in the environment
    pub variables: VariableSet,

    /// Interface to the system-managed parts of the environment
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
            getopts_state: Default::default(),
            jobs: Default::default(),
            main_pgid: system.getpgrp(),
            main_pid: system.getpid(),
            options: Default::default(),
            stack: Default::default(),
            traps: Default::default(),
            tty: Default::default(),
            variables: Default::default(),
            system: SharedSystem::new(system),
        }
    }

    /// Creates a new environment with a default-constructed [`VirtualSystem`].
    #[must_use]
    pub fn new_virtual() -> Env {
        Env::with_system(Box::<VirtualSystem>::default())
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
            getopts_state: self.getopts_state.clone(),
            jobs: self.jobs.clone(),
            main_pgid: self.main_pgid,
            main_pid: self.main_pid,
            options: self.options,
            stack: self.stack.clone(),
            traps: self.traps.clone(),
            tty: self.tty,
            variables: self.variables.clone(),
            system: SharedSystem::new(system),
        }
    }

    /// Initializes default variables.
    ///
    /// This function assigns the following variables to `self`:
    ///
    /// - `IFS=' \t\n'`
    /// - `OPTIND=1`
    /// - `PS1='$ '`
    /// - `PS2='> '`
    /// - `PS4='+ '`
    /// - `PPID=(parent process ID)`
    /// - `PWD=(current working directory)` (See [`Env::prepare_pwd`])
    ///
    /// This function ignores any errors that may occur.
    pub fn init_variables(&mut self) {
        self.variables.init();

        self.variables
            .get_or_new("PPID", Scope::Global)
            .assign(self.system.getppid().to_string(), None)
            .ok();

        self.prepare_pwd().ok();
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

        let mut future = std::pin::pin!(self.wait_for_signals());

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

    /// Whether error messages should be printed in color
    ///
    /// This function decides whether messages printed to the standard error
    /// should contain ANSI color escape sequences. The result is true only if
    /// the standard error is a terminal.
    ///
    /// The current implementaion simply checks if the standard error is a
    /// terminal. This will be changed in the future to support user
    /// configuration.
    #[must_use]
    fn should_print_error_in_color(&self) -> bool {
        // TODO Enable color depending on user config (force/auto/never)
        // TODO Check if the terminal really supports color (needs terminfo)
        self.system.isatty(Fd::STDERR) == Ok(true)
    }

    /// Returns a file descriptor to the controlling terminal.
    ///
    /// This function returns `self.tty` if it is `Some` FD. Otherwise, it
    /// opens `/dev/tty` and saves the new FD to `self.tty` before returning it.
    pub fn get_tty(&mut self) -> nix::Result<Fd> {
        if let Some(fd) = self.tty {
            return Ok(fd);
        }

        let first_fd = self.system.open(
            CStr::from_bytes_with_nul(b"/dev/tty\0").unwrap(),
            crate::system::OFlag::O_RDWR | crate::system::OFlag::O_CLOEXEC,
            crate::system::Mode::empty(),
        )?;
        let final_fd = self.system.move_fd_internal(first_fd);
        self.tty = final_fd.ok();
        final_fd
    }

    /// Tests whether the shell is performing job control.
    ///
    /// This function returns true if and only if:
    ///
    /// - the [`Monitor`] option is `On` in `self.options`, and
    /// - the current context is not in a subshell (no `Frame::Subshell` in `self.stack`).
    #[must_use]
    pub fn controls_jobs(&self) -> bool {
        self.options.get(Monitor) == On && !self.stack.contains(&Frame::Subshell)
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
    ///
    /// If the target subshell is not job-controlled, you may want to use
    /// [`wait_for_subshell_to_finish`](Self::wait_for_subshell_to_finish)
    /// instead.
    pub async fn wait_for_subshell(&mut self, target: Pid) -> nix::Result<WaitStatus> {
        // We need to set the signal handling before calling `wait` so we don't
        // miss any `SIGCHLD` that may arrive between `wait` and `wait_for_signal`.
        self.traps.enable_sigchld_handler(&mut self.system)?;

        loop {
            if let Some((pid, state)) = self.system.wait(target)? {
                let status = state.to_wait_status(pid);
                self.jobs.update_status(status);
                return Ok(status);
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
        while let Ok(Some((pid, state))) = self.system.wait(Pid::from_raw(-1)) {
            self.jobs.update_status(state.to_wait_status(pid));
        }
    }

    /// Get an existing variable or create a new one.
    ///
    /// This method is a thin wrapper around [`VariableSet::get_or_new`].
    /// If the [`AllExport`] option is on, the variable is
    /// [exported](VariableRefMut::export) before being returned from the
    /// method.
    ///
    /// You should prefer using this method over [`VariableSet::get_or_new`] to
    /// make sure that the [`AllExport`] option is applied.
    pub fn get_or_create_variable<S>(&mut self, name: S, scope: Scope) -> VariableRefMut
    where
        S: Into<String>,
    {
        let mut variable = self.variables.get_or_new(name, scope);
        if self.options.get(AllExport) == On {
            variable.export(true);
        }
        variable
    }

    pub(crate) fn errexit_is_applicable(&self) -> bool {
        self.options.get(ErrExit) == On && !self.stack.contains(&Frame::Condition)
    }

    /// Returns a `Divert` if the shell should exit because of the `ErrExit`
    /// [shell option](self::option::Option).
    ///
    /// The function returns `Break(Divert::Exit)` if the `ErrExit` option is
    /// on, the current `self.exit_status` is non-zero, and the current stack
    /// has no `Condition` [frame](Frame); otherwise, `Continue(())`.
    pub fn apply_errexit(&self) -> ControlFlow<Divert> {
        if !self.exit_status.is_successful() && self.errexit_is_applicable() {
            Break(Divert::Exit(None))
        } else {
            Continue(())
        }
    }

    /// Updates the exit status from the given result.
    ///
    /// If `result` is a `Break(divert)` where `divert.exit_status()` is `Some`
    /// exit status, this function sets `self.exit_status` to that exit status.
    pub fn apply_result(&mut self, result: crate::semantics::Result) {
        match result {
            Continue(_) => {}
            Break(divert) => {
                if let Some(exit_status) = divert.exit_status() {
                    self.exit_status = exit_status;
                }
            }
        }
    }
}

pub mod builtin;
pub mod function;
pub mod input;
pub mod io;
pub mod job;
pub mod option;
pub mod pwd;
pub mod semantics;
pub mod stack;
pub mod subshell;
pub mod system;
pub mod trap;
pub mod variable;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MIN_INTERNAL_FD;
    use crate::job::Job;
    use crate::job::ProcessState;
    use crate::subshell::Subshell;
    use crate::system::r#virtual::INode;
    use crate::system::r#virtual::SystemState;
    use crate::system::Errno;
    use crate::trap::Action;
    use futures_executor::LocalPool;
    use futures_util::task::LocalSpawnExt as _;
    use futures_util::FutureExt as _;
    use std::cell::RefCell;
    use yash_syntax::source::Location;

    /// Helper function to perform a test in a virtual system with an executor.
    pub fn in_virtual_system<F, Fut, T>(f: F) -> T
    where
        F: FnOnce(Env, Rc<RefCell<SystemState>>) -> Fut,
        Fut: Future<Output = T> + 'static,
        T: 'static,
    {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut executor = futures_executor::LocalPool::new();
        state.borrow_mut().executor = Some(Rc::new(executor.spawner()));

        let env = Env::with_system(Box::new(system));
        let shared_system = env.system.clone();
        let task = f(env, Rc::clone(&state));
        let mut task = executor.spawner().spawn_local_with_handle(task).unwrap();
        loop {
            if let Some(result) = (&mut task).now_or_never() {
                return result;
            }
            executor.run_until_stalled();
            shared_system.select(false).unwrap();
            SystemState::select_all(&state);
        }
    }

    #[test]
    fn wait_for_signal_remembers_signal_in_trap_set() {
        in_virtual_system(|mut env, state| async move {
            env.traps
                .set_action(
                    &mut env.system,
                    Signal::SIGCHLD,
                    Action::Command("".into()),
                    Location::dummy(""),
                    false,
                )
                .unwrap();
            {
                let mut state = state.borrow_mut();
                let process = state.processes.get_mut(&env.main_pid).unwrap();
                assert!(process.blocked_signals().contains(Signal::SIGCHLD));
                let _ = process.raise_signal(Signal::SIGCHLD);
            }
            env.wait_for_signal(Signal::SIGCHLD).await;

            let trap_state = env.traps.get_state(Signal::SIGCHLD).0.unwrap();
            assert!(trap_state.pending);
        })
    }

    fn poll_signals_env() -> (Env, VirtualSystem) {
        let system = VirtualSystem::new();
        let shared_system = SharedSystem::new(Box::new(system.clone()));
        let mut env = Env::with_system(Box::new(shared_system));
        env.traps
            .set_action(
                &mut env.system,
                Signal::SIGCHLD,
                Action::Command("".into()),
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
            let _ = process.raise_signal(Signal::SIGCHLD);
        }

        let result = env.poll_signals().unwrap();
        assert_eq!(*result, [Signal::SIGCHLD]);
    }

    #[test]
    fn get_tty_opens_tty() {
        let system = VirtualSystem::new();
        let tty = Rc::new(RefCell::new(INode::new([])));
        system
            .state
            .borrow_mut()
            .file_system
            .save("/dev/tty", Rc::clone(&tty))
            .unwrap();
        let mut env = Env::with_system(Box::new(system.clone()));

        let fd = env.get_tty().unwrap();
        assert!(
            fd >= MIN_INTERNAL_FD,
            "get_tty returned {fd}, which should be >= {MIN_INTERNAL_FD}"
        );
        system
            .with_open_file_description(fd, |ofd| {
                assert!(Rc::ptr_eq(&ofd.file, &tty));
                Ok(())
            })
            .unwrap();

        system.state.borrow_mut().file_system = Default::default();

        // get_tty returns cached FD
        let fd = env.get_tty().unwrap();
        system
            .with_open_file_description(fd, |ofd| {
                assert!(Rc::ptr_eq(&ofd.file, &tty));
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn start_and_wait_for_subshell() {
        in_virtual_system(|mut env, _state| async move {
            let subshell = Subshell::new(|env, _job_control| {
                Box::pin(async move {
                    env.exit_status = ExitStatus(42);
                    Continue(())
                })
            });
            let (pid, _) = subshell.start(&mut env).await.unwrap();
            let result = env.wait_for_subshell(pid).await;
            assert_eq!(result, Ok(WaitStatus::Exited(pid, 42)));
        });
    }

    #[test]
    fn start_and_wait_for_subshell_with_job_set() {
        in_virtual_system(|mut env, _state| async move {
            let subshell = Subshell::new(|env, _job_control| {
                Box::pin(async move {
                    env.exit_status = ExitStatus(42);
                    Continue(())
                })
            });
            let (pid, _) = subshell.start(&mut env).await.unwrap();
            let mut job = Job::new(pid);
            job.name = "my job".to_string();
            let job_index = env.jobs.add(job.clone());
            let result = env.wait_for_subshell(pid).await;
            assert_eq!(result, Ok(WaitStatus::Exited(pid, 42)));
            job.state = ProcessState::Exited(ExitStatus(42));
            assert_eq!(env.jobs.get(job_index), Some(&job));
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

        let [job_1, job_2, job_3] = executor.run_until(async {
            // Run a subshell.
            let subshell_1 = Subshell::new(|env, _job_control| {
                Box::pin(async move {
                    env.exit_status = ExitStatus(12);
                    Continue(())
                })
            });
            let (pid_1, _) = subshell_1.start(&mut env).await.unwrap();

            // Run another subshell.
            let subshell_2 = Subshell::new(|env, _job_control| {
                Box::pin(async move {
                    env.exit_status = ExitStatus(35);
                    Continue(())
                })
            });
            let (pid_2, _) = subshell_2.start(&mut env).await.unwrap();

            // This one will never finish.
            let subshell_3 =
                Subshell::new(|_env, _job_control| Box::pin(futures_util::future::pending()));
            let (pid_3, _) = subshell_3.start(&mut env).await.unwrap();

            // Yet another subshell. We don't make this into a job.
            let subshell_4 = Subshell::new(|env, _job_control| {
                Box::pin(async move {
                    env.exit_status = ExitStatus(100);
                    Continue(())
                })
            });
            let (_pid_4, _) = subshell_4.start(&mut env).await.unwrap();

            // Create jobs.
            let job_1 = env.jobs.add(Job::new(pid_1));
            let job_2 = env.jobs.add(Job::new(pid_2));
            let job_3 = env.jobs.add(Job::new(pid_3));
            [job_1, job_2, job_3]
        });

        // Let the jobs (except job_3) finish.
        executor.run_until_stalled();

        // We're not yet updated.
        assert_eq!(env.jobs.get(job_1).unwrap().state, ProcessState::Running);
        assert_eq!(env.jobs.get(job_2).unwrap().state, ProcessState::Running);
        assert_eq!(env.jobs.get(job_3).unwrap().state, ProcessState::Running);

        env.update_all_subshell_statuses();

        // Now we have the results.
        assert_eq!(
            env.jobs.get(job_1).unwrap().state,
            ProcessState::Exited(ExitStatus(12))
        );
        assert_eq!(
            env.jobs.get(job_2).unwrap().state,
            ProcessState::Exited(ExitStatus(35))
        );
        assert_eq!(env.jobs.get(job_3).unwrap().state, ProcessState::Running);
    }

    #[test]
    fn get_or_create_variable_with_all_export_off() {
        let mut env = Env::new_virtual();
        let mut a = env.get_or_create_variable("a", Scope::Global);
        assert!(!a.is_exported);
        a.export(true);
        let a = env.get_or_create_variable("a", Scope::Global);
        assert!(a.is_exported);
    }

    #[test]
    fn get_or_create_variable_with_all_export_on() {
        let mut env = Env::new_virtual();
        env.options.set(AllExport, On);
        let mut a = env.get_or_create_variable("a", Scope::Global);
        assert!(a.is_exported);
        a.export(false);
        let a = env.get_or_create_variable("a", Scope::Global);
        assert!(a.is_exported);
    }

    #[test]
    fn errexit_on() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus::FAILURE;
        env.options.set(ErrExit, On);
        assert_eq!(env.apply_errexit(), Break(Divert::Exit(None)));
    }

    #[test]
    fn errexit_with_zero_exit_status() {
        let mut env = Env::new_virtual();
        env.options.set(ErrExit, On);
        assert_eq!(env.apply_errexit(), Continue(()));
    }

    #[test]
    fn errexit_in_condition() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus::FAILURE;
        env.options.set(ErrExit, On);
        let env = env.push_frame(Frame::Condition);
        assert_eq!(env.apply_errexit(), Continue(()));
    }

    #[test]
    fn errexit_off() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus::FAILURE;
        assert_eq!(env.apply_errexit(), Continue(()));
    }

    #[test]
    fn apply_result_with_continue() {
        let mut env = Env::new_virtual();
        env.apply_result(Continue(()));
        assert_eq!(env.exit_status, ExitStatus::default());
    }

    #[test]
    fn apply_result_with_divert_without_exit_status() {
        let mut env = Env::new_virtual();
        env.apply_result(Break(Divert::Exit(None)));
        assert_eq!(env.exit_status, ExitStatus::default());
    }

    #[test]
    fn apply_result_with_divert_with_exit_status() {
        let mut env = Env::new_virtual();
        env.apply_result(Break(Divert::Exit(Some(ExitStatus(67)))));
        assert_eq!(env.exit_status, ExitStatus(67));
    }
}
