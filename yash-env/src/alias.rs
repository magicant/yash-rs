// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Types for defining aliases

use crate::Env;
use std::rc::Rc;

#[doc(no_inline)]
pub use yash_syntax::alias::{Alias, AliasSet, Glossary, HashEntry};

/// Allows to look up aliases in the environment.
///
/// This implementation delegates to `self.aliases`.
impl Glossary for Env {
    #[inline(always)]
    fn look_up(&self, name: &str) -> Option<Rc<Alias>> {
        self.aliases.look_up(name)
    }
    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }
}
