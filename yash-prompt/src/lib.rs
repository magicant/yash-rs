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

//! This library crate provides functionalities to show a command prompt.
//!
//! # Overview
//!
//! The `yash-prompt` crate provides command prompt support for the `yash`
//! shell. It includes functionalities to expand prompt strings and display them
//! interactively.
//!
//! [`Prompter`] is a decorator struct that wraps an inner input source and
//! displays a command prompt before reading input from the user. It can be
//! used to create an interactive shell prompt. The prompter internally uses
//! the following functions to expand prompt strings:
//!
//! - [`fetch_posix`]: Fetches the value of a variable defined by POSIX for
//!   a prompt string.
//! - [`expand_posix`]: Expands a prompt string in a POSIX-compliant manner.
//! - `expand_ex`: Expands a prompt string with yash-specific expansions.
//!   (This function is not yet implemented.)
//!
//! [`expand_posix`]: expand_posix()
//!
//! # Examples
//!
//! Construct an input source with a prompter and read input from the user:
//!
//! ```
//! # use futures_util::future::FutureExt as _;
//! # async {
//! use std::cell::RefCell;
//! use std::ops::ControlFlow::Continue;
//! use yash_env::Env;
//! use yash_env::input::FdReader;
//! use yash_env::io::Fd;
//! use yash_env::semantics::ExitStatus;
//! use yash_prompt::Prompter;
//! use yash_semantics::read_eval_loop;
//! use yash_syntax::parser::lex::Lexer;
//! use yash_syntax::source::Source;
//!
//! let mut env = Env::new_virtual();
//! let reader = FdReader::new(Fd::STDIN, env.system.clone());
//! let mut ref_env = RefCell::new(&mut env);
//! let input = Box::new(Prompter::new(reader, &ref_env));
//! let mut config = Lexer::config();
//! config.source = Some(Source::Stdin.into());
//! let mut lexer = config.input(input);
//! let result = read_eval_loop(&ref_env, &mut lexer).await;
//! drop(lexer);
//! assert_eq!(result, Continue(()));
//! assert_eq!(env.exit_status, ExitStatus::SUCCESS);
//! # }.now_or_never().unwrap();
//! ```

mod expand_posix;
pub use expand_posix::expand_posix;

// TODO Yash-specific prompt expansion

mod prompter;
pub use prompter::Prompter;
pub use prompter::fetch_posix;
