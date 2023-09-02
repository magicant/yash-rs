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

//! Shell startup

use self::args::Run;
use self::args::Source;
use std::cell::Cell;
use std::rc::Rc;
use thiserror::Error;
use yash_env::input::FdReader;
use yash_env::io::Fd;
use yash_env::option::Option::Interactive;
use yash_env::option::State;
use yash_env::system::Errno;
use yash_env::SharedSystem;
use yash_env::System;
#[cfg(doc)]
use yash_semantics::ReadEvalLoop;
use yash_syntax::input::Input;
use yash_syntax::input::Memory;
use yash_syntax::source::Source as SyntaxSource;

pub mod args;

/// Tests whether the shell should be implicitly interactive.
///
/// As per POSIX, "if there are no operands and the shell's standard input and
/// standard error are attached to a terminal, the shell is considered to be
/// interactive." This function implements this rule.
pub fn auto_interactive<S: System>(system: &S, run: &Run) -> bool {
    if run.source != Source::Stdin {
        return false;
    }
    if run.options.iter().any(|&(o, _)| o == Interactive) {
        return false;
    }
    if !run.positional_params.is_empty() {
        return false;
    }
    system.isatty(Fd::STDIN).unwrap_or(false) && system.isatty(Fd::STDERR).unwrap_or(false)
}

/// Result of [`prepare_input`].
pub struct SourceInput<'a> {
    /// Input to be passed to the parser.
    pub input: Box<dyn Input + 'a>,
    /// Source of the input.
    pub source: SyntaxSource,
    /// Flag that should be passed to [`ReadEvalLoop::set_verbose`].
    pub verbose: Option<Rc<Cell<State>>>,
}

/// Error returned by [`prepare_input`].
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cannot open script file '{path}': {errno}")]
pub struct PrepareInputError {
    /// Raw error value returned by the underlying system call.
    pub errno: Errno,
    /// Path of the script file that could not be opened.
    pub path: String,
}

/// Prepares the input for the shell.
pub fn prepare_input<'a>(
    system: &SharedSystem,
    source: &'a Source,
) -> Result<SourceInput<'a>, PrepareInputError> {
    match source {
        Source::Stdin => {
            let mut input = Box::new(FdReader::new(system.clone()));
            let echo = Rc::new(Cell::new(State::Off));
            input.set_echo(Some(Rc::clone(&echo)));
            Ok(SourceInput {
                input,
                source: SyntaxSource::Stdin,
                verbose: Some(echo),
            })
        }
        Source::File { .. } => todo!(),
        Source::String(command) => {
            let input = Box::new(Memory::new(command));
            Ok(SourceInput {
                input,
                source: SyntaxSource::CommandString,
                verbose: None,
            })
        }
    }
}
