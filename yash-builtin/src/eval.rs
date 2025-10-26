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
//! This module implements the [`eval` built-in], which evaluates the arguments
//! as shell commands.
//!
//! [`eval` built-in]: https://magicant.github.io/yash-rs/builtins/eval.html
//!
//! # Implementation notes
//!
//! The built-in requires a [`RunReadEvalLoop`] instance to be available in the
//! environment's [`any`](yash_env::Env::any) storage.

use crate::Result;
use crate::common::report::report_error;
use crate::common::syntax::{Mode, parse_arguments};
use std::cell::RefCell;
use std::rc::Rc;
use yash_env::Env;
#[cfg(doc)]
use yash_env::semantics::ExitStatus;
use yash_env::semantics::{Field, RunReadEvalLoop};
use yash_syntax::input::Memory;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Source;

/// Entry point of the `eval` built-in execution
///
/// This function implements the main logic of the `eval` built-in. It takes the
/// shell environment and the command-line arguments as input, joins the
/// arguments into a single command string, and evaluates the command string by
/// invoking the read-eval loop.
///
/// The function requires a [`RunReadEvalLoop`] instance to be available in the
/// environment's [`any`](Env::any) storage. This instance is used to run the
/// read-eval loop for evaluating the command string. If the instance is not
/// found, the function **panics**.
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO Support non-POSIX options
    let args = match parse_arguments(&[], Mode::with_env(env), args) {
        Ok((_options, operands)) => operands,
        Err(error) => return report_error(env, &error).await,
    };

    let command = match join(args) {
        Some(command) => command,
        None => return Result::default(),
    };

    // Parse and execute the command string
    let RunReadEvalLoop(run_read_eval_loop) = *env
        .any
        .get()
        .expect("`eval` built-in requires `RunReadEvalLoop` in `Env::any`");
    let mut config = Lexer::config();
    config.source = Some(Rc::new(Source::Eval {
        original: command.origin,
    }));
    let mut lexer = config.input(Box::new(Memory::new(&command.value)));
    let divert = run_read_eval_loop(&RefCell::new(env), &mut lexer).await;
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
