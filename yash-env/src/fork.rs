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

//! Helpers for [`Env::run_in_child_process`] and related functions

use super::Env;
use crate::alias::AliasSet;
use crate::any::DataSet;
use crate::builtin::Builtin;
use crate::function::FunctionSet;
use crate::io::Fd;
use crate::job::{JobList, Pid};
use crate::option::OptionSet;
use crate::semantics::ExitStatus;
use crate::stack::Stack;
use crate::system::Concurrent;
use crate::trap::TrapSet;
use crate::variable::VariableSet;
use std::collections::HashMap;
use std::mem::take;
use std::rc::Rc;

/// Subset of [`Env`] for optimizing child process creation
///
/// This contains a temporary extract of the `Env` struct for creating child
/// processes in [`Env::run_in_child_process`]. This is used to avoid cloning
/// the entire `Env` struct before forking, which may be expensive. The parent
/// and child processes will share the same `ForkEnvState` instance, avoiding
/// the need to clone it.
#[derive(Debug)]
pub struct ForkEnvState<S> {
    aliases: AliasSet,
    arg0: String,
    builtins: HashMap<&'static str, Builtin<S>>,
    exit_status: ExitStatus,
    functions: FunctionSet<S>,
    jobs: JobList,
    main_pgid: Pid,
    main_pid: Pid,
    options: OptionSet,
    stack: Stack,
    traps: TrapSet,
    tty: Option<Fd>,
    variables: VariableSet,
    any: DataSet,
}

impl<S> ForkEnvState<S> {
    /// Extracts a `ForkEnvState` from the given `Env`.
    ///
    /// This method is used internally by [`Env::run_in_child_process`] in the
    /// parent process to create a `ForkEnvState` instance for the child
    /// process.
    ///
    /// This method [takes](std::mem::take) some fields of the `Env` struct and
    /// moves them into the `ForkEnvState` struct. The `Env` fields are replaced
    /// with default values and must be restored by calling
    /// [`restore_into_env`](Self::restore_into_env) before the `Env` instance
    /// is used again.
    #[must_use]
    pub fn extract_from_env(env: &mut Env<S>) -> Self {
        Self {
            aliases: take(&mut env.aliases),
            arg0: take(&mut env.arg0),
            builtins: take(&mut env.builtins),
            exit_status: env.exit_status,
            functions: take(&mut env.functions),
            jobs: take(&mut env.jobs),
            main_pgid: env.main_pgid,
            main_pid: env.main_pid,
            options: env.options,
            stack: take(&mut env.stack),
            traps: take(&mut env.traps),
            tty: env.tty,
            variables: take(&mut env.variables),
            any: take(&mut env.any),
        }
    }

    /// Puts the fields of this `ForkEnvState` back into the given `Env`.
    ///
    /// This method is used internally by [`Env::run_in_child_process`] in the
    /// parent process to restore the `Env` struct after forking. It should be
    /// called after the child process has been created and the `ForkEnvState`
    /// instance is no longer needed.
    pub fn restore_into_env(self, env: &mut Env<S>) {
        // Decompose `self` to make sure we handle all fields
        let Self {
            aliases,
            arg0,
            builtins,
            exit_status,
            functions,
            jobs,
            main_pgid,
            main_pid,
            options,
            stack,
            traps,
            tty,
            variables,
            any,
        } = self;

        env.aliases = aliases;
        env.arg0 = arg0;
        env.builtins = builtins;
        env.exit_status = exit_status;
        env.functions = functions;
        env.jobs = jobs;
        env.main_pgid = main_pgid;
        env.main_pid = main_pid;
        env.options = options;
        env.stack = stack;
        env.traps = traps;
        env.tty = tty;
        env.variables = variables;
        env.any = any;
    }

    /// Creates a new `Env` instance with the same fields as this `ForkEnvState`
    /// and the given system.
    ///
    /// This method is used internally by [`Env::run_in_child_process`] in the
    /// child process to create an `Env` instance for running the child task.
    /// The child process will call this method with the same `ForkEnvState`
    /// instance that was created in the parent process, so the child process
    /// will have the same environment as the parent.
    #[must_use]
    pub fn into_env_with_system(self, system: Rc<Concurrent<S>>) -> Env<S> {
        Env {
            aliases: self.aliases,
            arg0: self.arg0,
            builtins: self.builtins,
            exit_status: self.exit_status,
            functions: self.functions,
            jobs: self.jobs,
            main_pgid: self.main_pgid,
            main_pid: self.main_pid,
            options: self.options,
            stack: self.stack,
            traps: self.traps,
            tty: self.tty,
            variables: self.variables,
            any: self.any,
            system,
        }
    }
}

// Instead of deriving `Clone`, we implement it manually to make sure it is
// implemented for all `S`, even if `S` does not implement `Clone`.
impl<S> Clone for ForkEnvState<S> {
    fn clone(&self) -> Self {
        Self {
            aliases: self.aliases.clone(),
            arg0: self.arg0.clone(),
            builtins: self.builtins.clone(),
            exit_status: self.exit_status,
            functions: self.functions.clone(),
            jobs: self.jobs.clone(),
            main_pgid: self.main_pgid,
            main_pid: self.main_pid,
            options: self.options,
            stack: self.stack.clone(),
            traps: self.traps.clone(),
            tty: self.tty,
            variables: self.variables.clone(),
            any: self.any.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.aliases.clone_from(&source.aliases);
        self.arg0.clone_from(&source.arg0);
        self.builtins.clone_from(&source.builtins);
        self.exit_status = source.exit_status;
        self.functions.clone_from(&source.functions);
        self.jobs.clone_from(&source.jobs);
        self.main_pgid = source.main_pgid;
        self.main_pid = source.main_pid;
        self.options = source.options;
        self.stack.clone_from(&source.stack);
        self.traps.clone_from(&source.traps);
        self.tty = source.tty;
        self.variables.clone_from(&source.variables);
        self.any.clone_from(&source.any);
    }
}
