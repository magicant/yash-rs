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

//! Command search
//!
//! This module provides the search functionality of the `command` built-in.
//! It is based on the [`yash_semantics::command_search`] module, but it adds
//! the ability to select the category of the command to search for.

use super::Search;
use std::ffi::CStr;
use yash_env::builtin::Builtin;
use yash_env::function::FunctionSet;
use yash_env::variable::Variable;
use yash_env::Env;

/// Environment adapter for applying the search parameters
///
/// This type implements the [`yash_semantics::command_search::SearchEnv`] trait
/// by extracting results from the environment filtered by the search
/// parameters.
#[derive(Clone, Copy, Debug)]
pub struct SearchEnv<'a> {
    pub env: &'a Env,
    pub params: &'a Search,
}

impl yash_semantics::command_search::PathEnv for SearchEnv<'_> {
    fn path(&self) -> Option<&Variable> {
        // TODO Seems like this trait function needs to be changed to return
        // something more abstract so that we can return the result of confstr.
        todo!()
    }

    #[inline]
    fn is_executable_file(&self, path: &CStr) -> bool {
        self.env.is_executable_file(path)
    }
}

impl yash_semantics::command_search::SearchEnv for SearchEnv<'_> {
    fn builtin(&self, name: &str) -> Option<Builtin> {
        todo!("return built-in {name}")
    }

    fn functions(&self) -> &FunctionSet {
        // TODO Can we change this trait function to take a function name and
        // return a reference to the function?
        todo!()
    }
}
