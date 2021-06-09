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

//! Shell language syntax and parser.
//!
//! This crate defines data types for constructing abstract syntax trees (AST)
//! of the shell language. See the [`syntax`] module for details.
//!
//! To parse source code into an AST, you can use the `parse` function on a
//! `&str`, which is allowed by the implementations of
//! [`FromStr`](std::str::FromStr) for the AST data types.
//! However, ASTs constructed in this way do not contain very meaningful
//! [source](crate::source) information.  All
//! [location](crate::source::Location)s in the ASTs only have [unknown
//! source](crate::source::Source::Unknown).
//!
//! To include a proper source information, you need to prepare a
//! [lexer](crate::parser::lex::Lexer) with source information and then pass it
//! to a parser. See the [`parser`] module for details.

pub mod input;
pub mod parser;
pub mod syntax;

pub use yash_core::alias;
pub use yash_core::env;
pub use yash_core::source;
