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
use std::cell::RefCell;
use std::ffi::CString;
use thiserror::Error;
use yash_env::input::Echo;
use yash_env::input::FdReader;
use yash_env::io::Fd;
use yash_env::option::Option::Interactive;
use yash_env::option::State::On;
use yash_env::system::Errno;
use yash_env::system::Mode;
use yash_env::system::OFlag;
use yash_env::system::SystemEx as _;
use yash_env::Env;
use yash_env::System;
use yash_prompt::Prompter;
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
    /// Input to be passed to the lexer
    pub input: Box<dyn Input + 'a>,
    /// Description of the source
    pub source: SyntaxSource,
}

/// Error returned by [`prepare_input`].
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cannot open script file '{path}': {errno}")]
pub struct PrepareInputError<'a> {
    /// Raw error value returned by the underlying system call.
    pub errno: Errno,
    /// Path of the script file that could not be opened.
    pub path: &'a str,
}

/// Prepares the input for the shell.
///
/// This function constructs an input object from the given source.
/// If the shell is interactive and the source is the standard input,
/// the [`Prompter`] decorator is applied to the input to show the prompt.
/// If the source is read with a file descriptor, the [`Echo`] decorator is
/// applied to the input to implement the [verbose](yash_env::option::Verbose)
/// shell option.
///
/// The `RefCell` passed as the first argument should be shared with (and only
/// with) the [`read_eval_loop`](yash_semantics::read_eval_loop) function that
/// consumes the input and executes the parsed commands.
pub fn prepare_input<'a>(
    env: &'a RefCell<&mut Env>,
    source: &'a Source,
) -> Result<SourceInput<'a>, PrepareInputError<'a>> {
    match source {
        Source::Stdin => {
            let (mut system, is_interactive) = {
                let env = env.borrow();
                (env.system.clone(), env.options.get(Interactive) == On)
            };

            if system.isatty(Fd::STDIN).unwrap_or(false) || system.fd_is_pipe(Fd::STDIN) {
                // It makes virtually no sense to make it blocking here
                // since we will be doing non-blocking reads anyway,
                // but POSIX requires us to do it.
                // https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/utilities/sh.html#tag_20_117_06
                system.set_blocking(Fd::STDIN).ok();
            }

            let reader = FdReader::new(Fd::STDIN, system);
            let input: Box<dyn Input> = if is_interactive {
                Box::new(Echo::new(Prompter::new(reader, env), env))
            } else {
                Box::new(Echo::new(reader, env))
            };
            let source = SyntaxSource::Stdin;
            Ok(SourceInput { input, source })
        }

        Source::File { path } => {
            let mut system = env.borrow().system.clone();

            let c_path = CString::new(path.as_str()).map_err(|_| PrepareInputError {
                errno: Errno::EILSEQ,
                path,
            })?;
            let fd = system
                .open(&c_path, OFlag::O_RDONLY | OFlag::O_CLOEXEC, Mode::empty())
                .and_then(|fd| system.move_fd_internal(fd))
                .map_err(|errno| PrepareInputError { errno, path })?;

            // TODO Make FdReader buffered
            let input = Box::new(Echo::new(FdReader::new(fd, system), env));
            let path = path.to_owned();
            let source = SyntaxSource::CommandFile { path };
            Ok(SourceInput { input, source })
        }

        Source::String(command) => {
            let input = Box::new(Memory::new(command));
            let source = SyntaxSource::CommandString;
            Ok(SourceInput { input, source })
        }
    }
}
