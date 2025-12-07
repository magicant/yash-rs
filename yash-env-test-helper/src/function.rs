// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! Test helpers for [`yash_env::function`]

use std::rc::Rc;
use yash_env::Env;
use yash_env::function::{FunctionBody, FunctionBodyObject};

/// A stub implementation of [`FunctionBody`] that panics on use.
#[derive(Clone, Debug, Default)]
pub struct FunctionBodyStub;

impl FunctionBodyStub {
    /// Creates a new [`FunctionBodyStub`].
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Creates a new [`FunctionBodyStub`] contained in an [`Rc`].
    ///
    /// Suitable for passing to [`yash_env::function::Function::new`].
    #[inline]
    #[must_use]
    pub fn rc_dyn<S>() -> Rc<dyn FunctionBodyObject<S>> {
        Rc::new(Self::new())
    }
}

/// Always panics when formatted.
impl std::fmt::Display for FunctionBodyStub {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unreachable!("unexpected call to FunctionBodyStub::fmt")
    }
}

/// Always panics when executed.
impl<S> FunctionBody<S> for FunctionBodyStub {
    async fn execute(&self, _: &mut Env<S>) -> yash_env::semantics::Result {
        unreachable!("unexpected call to FunctionBodyStub::execute")
    }
}
