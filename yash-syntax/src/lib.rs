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
//! Some AST elements (e.g. [`Word`](syntax::Word)) provide a
//! [location](source::Location) where the element appears in the source code.
//! See the [`source`] module to learn how locations are coded in this crate.
//!
//! To parse source code into an AST, you can use the `parse` function on a
//! `&str`, which is enabled by the implementations of
//! [`FromStr`](std::str::FromStr) for the AST data types. However, ASTs
//! constructed this way do not contain very meaningful source information: All
//! locations' source will be [unknown](source::Source::Unknown). To include
//! substantial source information, you need to prepare a
//! [lexer](parser::lex::Lexer) with source information and then pass it to a
//! [parser](parser::Parser). See the [`parser`] module for details.
//!
//! The [`input`] module defines an abstract method for feeding the parser with
//! source code.
//!
//! This crate also defines the [`alias`] module that can be used to define
//! aliases that are recognized while parsing.

pub mod alias;
pub mod input;
pub mod parser;
pub mod source;
pub mod syntax;
