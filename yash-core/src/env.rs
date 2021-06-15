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

/// Whole shell execution environment.
pub trait Enx: AliasEnv + BuiltinEnv {}

/// Whole shell execution environment.
///
/// The shell execution environment consists of application-managed parts and
/// system-managed parts. Application-managed parts are directly implemented in
/// the `Env` instance. System-managed parts are... TODO Elaborate
#[derive(Clone, Debug)]
pub struct Env {
    /// Aliases defined in the environment.
    ///
    /// The `AliasSet` is reference-counted so that the shell can execute traps
    /// while the parser is reading a command line.
    pub aliases: Rc<AliasSet>,

    /// Built-in utilities available in the environment.
    pub builtins: HashMap<&'static str, Builtin>,
}

impl AliasEnv for Env {
    fn aliases(&self) -> &Rc<AliasSet> {
        &self.aliases
    }
    fn aliases_mut(&mut self) -> &mut Rc<AliasSet> {
        &mut self.aliases
    }
}

impl BuiltinEnv for Env {
    fn builtin(&self, name: &str) -> Option<&Builtin> {
        self.builtins.get(name)
    }
}

impl Enx for Env {}
