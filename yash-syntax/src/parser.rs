// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2020 WATANABE Yuki
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

//! Syntax parser for the shell language.
//!
//! TODO Elaborate

mod core;
mod fill;
mod fromstr;

mod and_or;
mod case;
mod command;
mod compound_command;
mod for_loop;
mod function;
mod grouping;
mod list;
mod pipeline;
mod redir;
mod simple_command;
mod while_loop;

pub mod lex;

pub use self::core::AsyncFnMut;
pub use self::core::Error;
pub use self::core::ErrorCause;
pub use self::core::Parser;
pub use self::core::Rec;
pub use self::core::Result;
pub use self::core::SyntaxError;
pub use self::fill::Fill;
pub use self::fill::MissingHereDoc;
