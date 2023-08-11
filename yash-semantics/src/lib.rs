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

//! Semantics of the shell language.
//!
//! This crate defines the standard semantics for the shell language. The core
//! of the semantics is command execution and word expansion.
//! A command can be executed by calling
//! [`Command::execute`](command::Command::execute).
//! A word can be expanded by using functions and traits defined in
//! [`expansion`].
//!
//! A [`ReadEvalLoop`] reads, parses, and executes commands from an
//! input. It is a utility for running a shell script.

pub mod assign;
pub mod command;
pub mod command_search;
pub mod expansion;
pub mod redir;
pub mod trap;
pub mod xtrace;

#[doc(no_inline)]
pub use yash_env::semantics::*;

mod handle;
pub use handle::Handle;

mod runner;
pub use runner::ReadEvalLoop;

#[cfg(test)]
pub(crate) mod tests;
