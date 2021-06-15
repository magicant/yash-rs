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

//! Shell execution environment.
//!
//! A shell execution environment is a collection of data that may affect or be
//! affected by execution of commands.
//!
//! TODO Elaborate

use crate::alias::AliasSet;
use crate::builtin::Builtin;
use std::collections::HashMap;
use std::rc::Rc;

/// Alias-related part of the shell execution environment.
pub trait AliasEnv {
    /// Returns a reference to the alias set.
    fn aliases(&self) -> &Rc<AliasSet>;
    /// Returns a mutable reference to the alias set.
    fn aliases_mut(&mut self) -> &mut Rc<AliasSet>;
}

/// Minimal implementor of [`AliasEnv`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Aliases(pub Rc<AliasSet>);

impl AliasEnv for Aliases {
    fn aliases(&self) -> &Rc<AliasSet> {
        &self.0
    }
    fn aliases_mut(&mut self) -> &mut Rc<AliasSet> {
        &mut self.0
    }
}

/// Part of the shell execution environment that is related with built-in
/// utilities.
pub trait BuiltinEnv {
    /// Returns a reference to the built-in for the specified name.
    fn builtin(&self, name: &str) -> Option<&Builtin>;
}

/// Minimal implementor of [`BuiltinEnv`].
#[derive(Clone, Debug)]
pub struct Builtins(pub HashMap<&'static str, Builtin>);

impl BuiltinEnv for Builtins {
    fn builtin(&self, name: &str) -> Option<&Builtin> {
        self.0.get(name)
    }
}

/// Subset of the shell execution environment that can be implemented
/// independently of the underlying OS features.
#[derive(Clone, Debug)]
pub struct LocalEnv {
    pub aliases: Aliases,
    pub builtins: Builtins,
}

impl LocalEnv {
    /// Creates a new local environment.
    #[allow(clippy::new_without_default)]
    pub fn new() -> LocalEnv {
        let aliases = Aliases(Rc::new(AliasSet::new()));
        let builtins = Builtins(HashMap::new());
        LocalEnv { aliases, builtins }
    }
}

impl AliasEnv for LocalEnv {
    fn aliases(&self) -> &Rc<AliasSet> {
        self.aliases.aliases()
    }
    fn aliases_mut(&mut self) -> &mut Rc<AliasSet> {
        self.aliases.aliases_mut()
    }
}

impl BuiltinEnv for LocalEnv {
    fn builtin(&self, name: &str) -> Option<&Builtin> {
        self.builtins.builtin(name)
    }
}

/// Whole shell execution environment.
pub trait Enx: AliasEnv + BuiltinEnv {}

/// Implementation of [`Enx`] that is based on the state of the current process.
#[derive(Debug)]
pub struct NativeEnv {
    /// Local part of the environment.
    pub local: LocalEnv,
}

impl NativeEnv {
    /// Creates a new environment.
    ///
    /// Because `NativeEnv` is tied with the state of the current process, there
    /// should be at most one instance of `NativeEnv` in a process. Using more
    /// than one `NativeEnv` instance at the same time should be considered
    /// unsafe.
    #[allow(clippy::new_without_default)]
    pub fn new() -> NativeEnv {
        let local = LocalEnv::new();
        NativeEnv { local }
    }
}

impl AliasEnv for NativeEnv {
    fn aliases(&self) -> &Rc<AliasSet> {
        self.local.aliases()
    }
    fn aliases_mut(&mut self) -> &mut Rc<AliasSet> {
        self.local.aliases_mut()
    }
}

impl BuiltinEnv for NativeEnv {
    fn builtin(&self, name: &str) -> Option<&Builtin> {
        self.local.builtin(name)
    }
}

impl Enx for NativeEnv {}

/// Simulated shell execution environment.
///
/// TODO Elaborate
#[derive(Clone, Debug)]
pub struct SimEnv {
    /// Local part of the environment.
    pub local: LocalEnv,
}

impl SimEnv {
    /// Creates a new `SimEnv`.
    #[allow(clippy::new_without_default)]
    pub fn new() -> SimEnv {
        let local = LocalEnv::new();
        SimEnv { local }
    }
}

impl AliasEnv for SimEnv {
    fn aliases(&self) -> &Rc<AliasSet> {
        self.local.aliases()
    }
    fn aliases_mut(&mut self) -> &mut Rc<AliasSet> {
        self.local.aliases_mut()
    }
}

impl BuiltinEnv for SimEnv {
    fn builtin(&self, name: &str) -> Option<&Builtin> {
        self.local.builtin(name)
    }
}

impl Enx for SimEnv {}
