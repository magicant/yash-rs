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

//! Preparing input for the parser
//!
//! This module implements the [`prepare_input`] function that prepares the
//! input for the shell syntax parser. The input is constructed from the given
//! source and decorated with the [`Echo`] and [`Prompter`] decorators as
//! necessary.
//!
//! [`PrepareInputError`] defines the error that may occur when preparing the
//! input.

use super::args::Source;
use std::cell::RefCell;
use std::ffi::CString;
use thiserror::Error;
use yash_env::Env;
use yash_env::input::Echo;
use yash_env::input::FdReader;
use yash_env::input::IgnoreEof;
use yash_env::input::Reporter;
use yash_env::io::Fd;
use yash_env::io::move_fd_internal;
use yash_env::option::Option::Interactive;
use yash_env::option::State::{Off, On};
use yash_env::parser::Config;
use yash_env::system::{
    Close, Dup, Errno, Fcntl, Fstat, Isatty, Mode, OfdAccess, Open, OpenFlag, Read, Signals, Write,
};
use yash_prompt::Prompter;
use yash_syntax::input::InputObject;
use yash_syntax::input::Memory;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Source as SyntaxSource;

/// Error returned by [`prepare_input`]
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cannot open script file '{path}': {errno}")]
pub struct PrepareInputError<'a> {
    /// Raw error value returned by the underlying system call.
    pub errno: Errno,
    /// Path of the script file that could not be opened.
    pub path: &'a str,
}

/// Prepares the input for the shell syntax parser.
///
/// This function constructs a lexer from the given source with the
/// following decorators applied to the input object:
///
/// - If the source is read with a file descriptor, the [`Echo`] decorator is
///   applied to the input to implement the [`Verbose`] shell option.
/// - If the [`Interactive`] option is enabled and the source is read with a
///   file descriptor, the [`Prompter`] decorator is applied to the input to
///   show the prompt.
/// - If the [`Interactive`] option is enabled, the [`Reporter`] decorator is
///   applied to the input to show changes in job status before prompting for
///   the next command.
/// - If the [`Interactive`] option is enabled and the source is read with a
///   file descriptor, the [`IgnoreEof`] decorator is applied to the input to
///   implement the [`IgnoreEof`](yash_env::option::IgnoreEof) shell option.
///
/// The `RefCell` passed as the first argument should be shared with (and only
/// with) the [`read_eval_loop`](yash_semantics::read_eval_loop) function that
/// consumes the input and executes the parsed commands.
///
/// [`Verbose`]: yash_env::option::Verbose
pub async fn prepare_input<'s, 'i, 'e, S>(
    env: &'i RefCell<&mut Env<S>>,
    source: &'s Source,
) -> Result<Lexer<'i>, PrepareInputError<'e>>
where
    's: 'i + 'e,
    S: Close + Dup + Fcntl + Fstat + Isatty + Open + Read + Signals + Write + 'static,
{
    fn lexer_with_input_and_source<'a>(
        input: Box<dyn InputObject + 'a>,
        source: SyntaxSource,
    ) -> Lexer<'a> {
        let mut config = Config::with_input(input);
        config.source = Some(source.into());
        config.into()
    }

    match source {
        Source::Stdin => {
            let system = env.borrow().system.clone();
            if system.isatty(Fd::STDIN) || system.fd_is_pipe(Fd::STDIN) {
                // It makes virtually no sense to make it blocking here
                // since we will be doing non-blocking reads anyway,
                // but POSIX requires us to do it.
                // https://pubs.opengroup.org/onlinepubs/9799919799/utilities/sh.html#tag_20_110_06
                _ = system.get_and_set_nonblocking(Fd::STDIN, false);
            }

            let input = prepare_fd_input(Fd::STDIN, env);
            let source = SyntaxSource::Stdin;
            Ok(lexer_with_input_and_source(input, source))
        }

        Source::File { path } => {
            let system = env.borrow().system.clone();

            let c_path = CString::new(path.as_str()).map_err(|_| PrepareInputError {
                errno: Errno::EILSEQ,
                path,
            })?;
            let fd = system
                .open(
                    &c_path,
                    OfdAccess::ReadOnly,
                    OpenFlag::CloseOnExec.into(),
                    Mode::empty(),
                )
                .await
                .and_then(|fd| move_fd_internal(&system, fd))
                .map_err(|errno| PrepareInputError { errno, path })?;

            let input = prepare_fd_input(fd, env);
            let path = path.to_owned();
            let source = SyntaxSource::CommandFile { path };
            Ok(lexer_with_input_and_source(input, source))
        }

        Source::String(command) => {
            let basic_input = Memory::new(command);

            let is_interactive = env.borrow().options.get(Interactive) == On;
            let input: Box<dyn InputObject> = if is_interactive {
                Box::new(Reporter::new(basic_input, env))
            } else {
                Box::new(basic_input)
            };
            let source = SyntaxSource::CommandString;
            Ok(lexer_with_input_and_source(input, source))
        }
    }
}

/// Creates an input object from a file descriptor.
///
/// This function creates an [`FdReader`] object from the given file descriptor
/// and wraps it with the [`Echo`] decorator. If the [`Interactive`] option is
/// enabled, the [`Prompter`], [`Reporter`], and [`IgnoreEof`] decorators are
/// applied to the input object.
fn prepare_fd_input<'i, S>(fd: Fd, ref_env: &'i RefCell<&mut Env<S>>) -> Box<dyn InputObject + 'i>
where
    S: Fcntl + Isatty + Read + Signals + Write + 'static,
{
    let env = ref_env.borrow();
    let system = env.system.clone();

    let basic_input = Echo::new(FdReader::new(fd, system), ref_env);

    if env.options.get(Interactive) == Off {
        Box::new(basic_input)
    } else {
        // The order of these decorators is important. The prompt should be shown after
        // the job status is reported, and both should be shown again if an EOF is ignored.
        let prompter = Prompter::new(basic_input, ref_env);
        let reporter = Reporter::new(prompter, ref_env);
        let message =
            "# Type `exit` to leave the shell when the ignore-eof option is on.\n".to_string();
        Box::new(IgnoreEof::new(reporter, fd, ref_env, message))
    }
}
