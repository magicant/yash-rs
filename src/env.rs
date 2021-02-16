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
//! TODO Elaborate

use crate::alias::AliasSet;
use std::rc::Rc;

/// Alias-related part of the shell execution environment.
pub trait AliasEnv {
    /// Returns a reference to the alias set.
    fn aliases(&self) -> &Rc<AliasSet>;
    /// Returns a mutable reference to the alias set.
    fn aliases_mut(&mut self) -> &mut Rc<AliasSet>;
}

/// Minimal implementor of [`AliasEnv`].
#[derive(Clone, Debug)]
pub struct Aliases(Rc<AliasSet>);

impl AliasEnv for Aliases {
    fn aliases(&self) -> &Rc<AliasSet> {
        &self.0
    }
    fn aliases_mut(&mut self) -> &mut Rc<AliasSet> {
        &mut self.0
    }
}

/// Whole shell execution environment.
pub trait Env: AliasEnv {}

/// Implementation of [`Env`] that is based on the state of the current process.
#[derive(Debug)]
pub struct NativeEnv {
    pub aliases: Aliases,
}

impl AliasEnv for NativeEnv {
    fn aliases(&self) -> &Rc<AliasSet> {
        self.aliases.aliases()
    }
    fn aliases_mut(&mut self) -> &mut Rc<AliasSet> {
        self.aliases.aliases_mut()
    }
}

impl Env for NativeEnv {}
