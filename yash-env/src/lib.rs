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
use std::collections::HashMap;
use std::ffi::CStr;
use std::fmt::Debug;
use std::rc::Rc;
use yash_syntax::alias::AliasSet;

/// Whole shell execution environment.
///
/// The shell execution environment consists of application-managed parts and
/// system-managed parts. Application-managed parts are directly implemented in
/// the `Env` instance. System-managed parts are abstracted as [`System`] so
/// that they can be replaced with a dummy implementation.
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
    pub system: Box<dyn System>,
}

/// Abstraction of the system-managed parts of the environment.
///
/// TODO Elaborate
pub trait System: Debug {
    /// Clones the `System` instance and returns it in a box.
    ///
    /// The semantics of cloning is determined by the implementor. Especially,
    /// a cloned [`RealSystem`] might render a surprising behavior.
    fn clone_box(&self) -> Box<dyn System>;

    /// Whether there is an executable file at the specified path.
    fn is_executable_file(&self, path: &CStr) -> bool;

    /// Clones the current shell process.
    ///
    /// This is a thin wrapper around the `fork` system call. Users of `Env`
    /// should not call it directly. Instead, use [`Env::new_subshell`] so that
    /// the environment can manage the created child process as a job member.
    ///
    /// # Safety
    ///
    /// See [nix's documentation](nix::unistd::fork) to learn why this function
    /// is unsafe.
    unsafe fn fork(&mut self) -> nix::Result<nix::unistd::ForkResult>;
}

// Auto-derived Clone cannot be used for this because `System` cannot be a
// super-trait of `Clone` as that would make the trait non-object-safe.
impl Clone for Box<dyn System> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
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

    /// TODO
    pub fn new_subshell(&mut self) {
        todo!()
    }
}
