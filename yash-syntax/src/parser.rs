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
//! The shell language parsing system has two important components: the lexical
//! analyzer and the syntax parser.
//!
//! The lexical analyzer, or simply [lexer](lex::Lexer), extracts tokens from
//! the source code string. Tokenization for the shell language involves parsing
//! expansions like parameter expansions and command substitutions, which makes
//! the lexer much more complicated than those for normal programming languages.
//! However, as long as you use the lexer indirectly via the parser, you don't
//! have to care about such details.
//!
//! The syntax [parser](Parser) examines tokens produced by the lexer and
//! constructs abstract syntax trees. The below code illustrates the basic usage
//! of the parser.
//!
//! ```
//! // First, prepare an input object that the lexer reads from.
//! use yash_syntax::input::Memory;
//! use yash_syntax::source::Source;
//! # // TODO demonstrate with a Source other than Unknown
//! let input = Box::new(Memory::new("echo $?"));
//!
//! // Next, create a lexer.
//! # use std::num::NonZeroU64;
//! # use yash_syntax::parser::lex::Lexer;
//! let line = NonZeroU64::new(1).unwrap();
//! let mut lexer = Lexer::new(input, line, Source::Unknown);
//!
//! // Then, create a new parser borrowing the lexer.
//! # use yash_syntax::parser::Parser;
//! # use yash_syntax::alias::AliasSet;
//! let aliases = AliasSet::new();
//! let mut parser = Parser::new(&mut lexer, &aliases);
//!
//! // Lastly, call the parser's function to get an AST.
//! use futures_executor::block_on;
//! let list = block_on(parser.command_line()).unwrap().unwrap();
//! assert_eq!(list.to_string(), "echo $?");
//! ```
//!
//! If there is any error reading the input or analyzing the source code, the
//! parser returns an [`Error`] object. In case of a syntax error, the `Error`
//! object's [cause](ErrorCause) will be a value of [`SyntaxError`] that
//! describes it.
//!
//! Most lexer and parser functions are asynchronous because underlying
//! [input](crate::input::Input) is asynchronous. Only as many lines are read
//! from the input as needed to parse a complete AST.
//!
//! Note that most AST types have the [`FromStr`](std::str::FromStr) trait
//! implemented for them. If you don't need to include source information in the
//! resultant AST, calling the `parse` function on a string is a convenient way
//! to parse a code fragment.
//! See the [`syntax`](crate::syntax) module for an example of this.

mod core;
mod error;
mod from_str;

mod and_or;
mod case;
mod command;
mod compound_command;
mod for_loop;
mod function;
mod grouping;
mod r#if;
mod list;
mod pipeline;
mod redir;
mod simple_command;
mod while_loop;

pub mod lex;

pub use self::core::Parser;
pub use self::core::Rec;
pub use self::core::Result;
pub use self::error::Error;
pub use self::error::ErrorCause;
pub use self::error::SyntaxError;
