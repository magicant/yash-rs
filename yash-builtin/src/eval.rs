// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Eval built-in
//!
//! The **`eval`** built-in evaluates the arguments as shell commands.
//!
//! # Synopsis
//!
//! ```sh
//! eval [command…]
//! ```
//!
//! # Description
//!
//! This built-in parses and executes the argument as a shell script in
//! the current shell environment.
//!
//! # Options
//!
//! None.
//!
//! (TODO: non-portable options)
//!
//! # Operands
//!
//! The operand is a command string to be evaluated.
//! If more than one operand is given, they are concatenated with spaces
//! between them to form a single command string.
//!
//! # Errors
//!
//! During parsing and execution, any syntax error or runtime error may
//! occur.
//!
//! # Exit status
//!
//! The exit status of the `eval` built-in is the exit status of the last
//! command executed in the command string.
//! If there is no command in the string, the exit status is zero.
//! In case of a syntax error, the exit status is 2 ([`ExitStatus::ERROR`]).
//!
//! # Portability
//!
//! POSIX does not require the eval built-in to conform to the Utility Syntax
//! Guidelines, which means portable scripts cannot use any options or the `--`
//! separator for the built-in.

use crate::Result;
use std::cell::RefCell;
use std::num::NonZeroU64;
use std::rc::Rc;
#[cfg(doc)]
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_semantics::read_eval_loop;
use yash_syntax::input::Memory;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Source;

/// Entry point of the `eval` built-in execution
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    let command = match join(args) {
        Some(command) => command,
        None => return Result::default(),
    };

    // Parse and execute the command string
    let input = Box::new(Memory::new(&command.value));
    let start_line_number = NonZeroU64::new(1).unwrap();
    let source = Rc::new(Source::Eval {
        original: command.origin,
    });
    let mut lexer = Lexer::new(input, start_line_number, source);
    let divert = read_eval_loop(&RefCell::new(env), &mut lexer).await;
    Result::with_exit_status_and_divert(env.exit_status, divert)
}

/// Joins the arguments to make a single command string.
fn join(args: Vec<Field>) -> Option<Field> {
    let mut args = args.into_iter();
    let mut command = args.next()?;
    command.value.reserve_exact(
        args.as_slice()
            .iter()
            .map(|arg| 1 + arg.value.len())
            .sum::<usize>(),
    );
    for arg in args {
        command.value.push(' ');
        command.value.push_str(&arg.value);
    }
    Some(command)
}
