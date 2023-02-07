// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Utility for starting subshells
//!
//! This module defines [`Subshell`], a builder for starting a subshell. It is
//! [constructed](Subshell::new) with a function you want to run in a subshell.
//! After configuring the builder with some options, you can
//! [start](Subshell::start) the subshell.
//!
//! [`Subshell`] is implemented as a wrapper around
//! [`System::new_child_process`]. You should prefer `Subshell` for the purpose
//! of creating a subshell because it helps to arrange the child process
//! properly.

use crate::job::Pid;
use crate::stack::Frame;
use crate::system::ChildProcessTask;
use crate::system::System;
use crate::Env;
use std::future::Future;
use std::pin::Pin;

/// Subshell builder
///
/// See the [module documentation](self) for details.
pub struct Subshell<F> {
    task: F,
}

impl<F> std::fmt::Debug for Subshell<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subshell").finish_non_exhaustive()
    }
}

impl<F> Subshell<F>
where
    F: for<'a> FnOnce(&'a mut Env) -> Pin<Box<dyn Future<Output = crate::semantics::Result> + 'a>>
        + 'static,
    // TODO Revisit to simplify this function type when impl Future is allowed in return type
{
    /// Creates a new subshell builder with a task.
    ///
    /// The task will run in a subshell after it is started.
    /// If the task returns an `Err(Divert::...)`, it is handled as follows:
    ///
    /// - `Interrupt` and `Exit` with `Some(exit_status)` override the exit
    ///   status in `Env`.
    /// - Other `Divert` values are ignored.
    pub fn new(task: F) -> Self {
        Subshell { task }
    }

    /// Starts a subshell.
    ///
    /// This function creates a new child process that runs the task contained
    /// in this builder. If the child was started successfully, this function
    /// returns the child's process ID. Otherwise, it returns an error.
    ///
    /// Although this function is `async`, it does not wait for the child to
    /// finish, which means the parent and child processes will run
    /// concurrently. To wait for the child to finish, you need to call
    /// [`Env::wait_for_subshell`] or [`Env::wait_for_subshell_to_finish`]. If
    /// job control is active, you may want to add the process ID to `env.jobs`
    /// before waiting.
    pub async fn start(self, env: &mut Env) -> nix::Result<Pid> {
        let task: ChildProcessTask = Box::new(move |env| {
            Box::pin(async move {
                let mut env = env.push_frame(Frame::Subshell);
                let env = &mut *env;
                env.traps.enter_subshell(&mut env.system);
                let result = (self.task)(env).await;
                env.apply_result(result);
            })
        });
        let child = env.system.new_child_process()?;
        let child_pid = child(env, task).await;
        Ok(child_pid)
    }
}
