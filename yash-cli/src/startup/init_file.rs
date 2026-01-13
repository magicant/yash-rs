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

//! Running initialization files
//!
//! This module provides functions for running initialization files in the shell.
//! The initialization file is a script that is executed when the shell starts up.
//!
//! Currently, this module only supports running the POSIX-defined rcfile, whose
//! path is determined by the value of the `ENV` environment variable.
//! (TODO: Support for yash-specific initialization files will be added later.)
//!
//! The [`run_rcfile`] function is the main entry point for running the rcfile.
//! Helper functions that are used by `run_rcfile` are also provided in this
//! module.

use super::args::InitFile;
use std::cell::RefCell;
use std::ffi::CString;
use std::rc::Rc;
use thiserror::Error;
use yash_env::Env;
use yash_env::input::{Echo, FdReader};
use yash_env::io::Fd;
use yash_env::io::move_fd_internal;
use yash_env::option::Option::Interactive;
use yash_env::option::State::Off;
use yash_env::parser::Config;
use yash_env::stack::Frame;
use yash_env::system::{
    Close, Dup, Errno, Fcntl, GetUid, Isatty, Mode, OfdAccess, Open, OpenFlag, Write,
};
use yash_env::variable::ENV;
use yash_semantics::expansion::expand_text;
use yash_semantics::read_eval_loop;
use yash_semantics::{Handle, Runtime};
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Source;

/// Errors that can occur when finding the default initialization file path
#[derive(Clone, Debug, Error, PartialEq)]
#[error(transparent)]
pub enum DefaultFilePathError {
    /// An error occurred while parsing the value of a variable specifying the
    /// initialization file path
    ParseError(#[from] yash_syntax::parser::Error),
    /// An error occurred while expanding the value of a variable specifying
    /// the initialization file path
    ExpansionError(#[from] yash_semantics::expansion::Error),
}

impl<S> Handle<S> for DefaultFilePathError
where
    S: Fcntl + Isatty + Write,
{
    async fn handle(&self, env: &mut Env<S>) -> yash_semantics::Result {
        match self {
            DefaultFilePathError::ParseError(e) => e.handle(env).await,
            DefaultFilePathError::ExpansionError(e) => e.handle(env).await,
        }
    }
}

/// Finds the path to the default rcfile.
///
/// The default path is determined by the value of the [`ENV`] environment
/// variable. The value is parsed as a [`Text`] and subjected to the
/// [initial expansion].
///
/// If the variable does not exist or is empty, the result will be an empty
/// string.
///
/// If the variable value cannot be parsed or expanded, an error message will
/// be printed to the standard error and an empty string will be returned.
///
/// TODO: If the POSIXly correct mode is off, the default path should be
/// `~/.yashrc` (or maybe some XDG-compliant path).
///
/// [`ENV`]: yash_env::variable::ENV
/// [`Text`]: yash_syntax::syntax::Text
/// [initial expansion]: yash_semantics::expansion::initial
pub async fn default_rcfile_path<S>(env: &mut Env<S>) -> Result<String, DefaultFilePathError>
where
    S: Runtime + 'static,
{
    let raw_value = env.variables.get_scalar(ENV).unwrap_or_default();

    let text = {
        let name = ENV.to_owned();
        let source = Source::VariableValue { name };
        let mut lexer = Lexer::from_memory(raw_value, source);
        lexer.text(|_| false, |_| false).await?
    };

    Ok(expand_text(env, &text).await?.0)
}

/// Resolves the path to the rcfile.
///
/// This function resolves the path to the rcfile specified by the `file`
/// argument. If the file is `InitFile::Default`, the default rcfile path is
/// determined by calling [`default_rcfile_path`].
///
/// This function returns an empty string in the following cases, in which
/// case the rcfile should not be executed:
///
/// - `file` is `InitFile::None`,
/// - the `Interactive` shell option is off,
/// - the real user ID of the process is not the same as the effective user ID, or
/// - the real group ID of the process is not the same as the effective group ID.
pub async fn resolve_rcfile_path<S>(
    env: &mut Env<S>,
    file: InitFile,
) -> Result<String, DefaultFilePathError>
where
    S: GetUid + Runtime + 'static,
{
    if file == InitFile::None
        || env.options.get(Interactive) == Off
        || env.system.getuid() != env.system.geteuid()
        || env.system.getgid() != env.system.getegid()
    {
        return Ok(String::default());
    }

    match file {
        InitFile::None => unreachable!(),
        InitFile::Default => default_rcfile_path(env).await,
        InitFile::File { path } => Ok(path),
    }
}

/// Runs an initialization file, reading from the specified path.
///
/// This function reads the contents of the initialization file and executes
/// them in the current shell environment. The file is specified by the `path`
/// argument.
///
/// If `path` is an empty string, the function returns immediately.
pub async fn run_init_file<S>(env: &mut Env<S>, path: &str)
where
    S: Runtime + 'static,
{
    if path.is_empty() {
        return;
    }

    fn open_fd<S>(system: &mut S, path: String) -> Result<Fd, Errno>
    where
        S: Close + Dup + Open,
    {
        let c_path = CString::new(path).map_err(|_| Errno::EILSEQ)?;
        let fd = system.open(
            &c_path,
            OfdAccess::ReadOnly,
            OpenFlag::CloseOnExec.into(),
            Mode::empty(),
        )?;
        move_fd_internal(system, fd)
    }

    let fd = match open_fd(&mut env.system, path.to_owned()) {
        Ok(fd) => fd,
        Err(errno) => {
            env.system
                .print_error(&format!(
                    "{}: cannot open initialization file {path:?}: {errno}\n",
                    &env.arg0
                ))
                .await;
            return;
        }
    };

    let env = &mut *env.push_frame(Frame::InitFile);
    let system = env.system.clone();
    let ref_env = RefCell::new(&mut *env);
    let input = Box::new(Echo::new(FdReader::new(fd, system), &ref_env));
    let mut config = Config::with_input(input);
    config.source = Some(Rc::new(Source::InitFile {
        path: path.to_owned(),
    }));
    let mut lexer = config.into();
    _ = read_eval_loop(&ref_env, &mut { lexer }).await;

    if let Err(errno) = env.system.close(fd) {
        env.system
            .print_error(&format!(
                "{}: cannot close initialization file {path:?}: {errno}\n",
                &env.arg0
            ))
            .await;
    }
}

/// Runs the rcfile specified by the `file` argument.
///
/// This function resolves the path to the rcfile using [`resolve_rcfile_path`]
/// and then runs the rcfile using [`run_init_file`]. Any errors resolving the
/// path are reported to the standard error.
pub async fn run_rcfile<S>(env: &mut Env<S>, file: InitFile)
where
    S: GetUid + Runtime + 'static,
{
    match resolve_rcfile_path(env, file).await {
        Ok(path) => run_init_file(env, &path).await,
        Err(e) => drop(e.handle(env).await),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use futures_util::FutureExt as _;
    use yash_env::VirtualSystem;
    use yash_env::option::State::On;
    use yash_env::system::{Gid, Uid};
    use yash_env::variable::Scope::Global;

    #[test]
    fn default_rcfile_path_with_unset_env() {
        let mut env = Env::new_virtual();
        let result = default_rcfile_path(&mut env).now_or_never().unwrap();
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn default_rcfile_path_with_empty_env() {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new(ENV, Global)
            .assign("", None)
            .unwrap();
        let result = default_rcfile_path(&mut env).now_or_never().unwrap();
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn default_rcfile_path_with_env_without_expansion() {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new(ENV, Global)
            .assign("foo", None)
            .unwrap();
        let result = default_rcfile_path(&mut env).now_or_never().unwrap();
        assert_eq!(result.unwrap(), "foo");
    }

    #[test]
    fn default_rcfile_path_with_env_with_unparsable_expansion() {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new(ENV, Global)
            .assign("foo${bar", None)
            .unwrap();
        let result = default_rcfile_path(&mut env).now_or_never().unwrap();
        assert_matches!(result, Err(DefaultFilePathError::ParseError(_)));
    }

    #[test]
    fn default_rcfile_path_with_env_with_failing_expansion() {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new(ENV, Global)
            .assign("${unset?}", None)
            .unwrap();
        let result = default_rcfile_path(&mut env).now_or_never().unwrap();
        assert_matches!(result, Err(DefaultFilePathError::ExpansionError(_)));
    }

    #[test]
    fn resolve_rcfile_path_none() {
        let mut env = Env::new_virtual();
        env.options.set(Interactive, On);
        let result = resolve_rcfile_path(&mut env, InitFile::None)
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn resolve_rcfile_path_default() {
        let mut env = Env::new_virtual();
        env.options.set(Interactive, On);
        env.variables
            .get_or_new(ENV, Global)
            .assign("foo/bar", None)
            .unwrap();
        let result = resolve_rcfile_path(&mut env, InitFile::Default)
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "foo/bar");
    }

    #[test]
    fn resolve_rcfile_path_exact() {
        let mut env = Env::new_virtual();
        env.options.set(Interactive, On);
        let path = "/path/to/rcfile".to_string();
        let file = InitFile::File { path };
        let result = resolve_rcfile_path(&mut env, file).now_or_never().unwrap();
        assert_eq!(result.unwrap(), "/path/to/rcfile");
    }

    #[test]
    fn resolve_rcfile_path_non_interactive() {
        let mut env = Env::new_virtual();
        env.options.set(Interactive, Off);
        let path = "/path/to/rcfile".to_string();
        let file = InitFile::File { path };
        let result = resolve_rcfile_path(&mut env, file).now_or_never().unwrap();
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn resolve_rcfile_path_non_real_user() {
        let system = VirtualSystem::new();
        system.current_process_mut().set_uid(Uid(0));
        system.current_process_mut().set_euid(Uid(10));
        let mut env = Env::with_system(system);
        env.options.set(Interactive, On);
        let path = "/path/to/rcfile".to_string();
        let file = InitFile::File { path };
        let result = resolve_rcfile_path(&mut env, file).now_or_never().unwrap();
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn resolve_rcfile_path_non_real_group() {
        let system = VirtualSystem::new();
        system.current_process_mut().set_gid(Gid(0));
        system.current_process_mut().set_egid(Gid(10));
        let mut env = Env::with_system(system);
        env.options.set(Interactive, On);
        let path = "/path/to/rcfile".to_string();
        let file = InitFile::File { path };
        let result = resolve_rcfile_path(&mut env, file).now_or_never().unwrap();
        assert_eq!(result.unwrap(), "");
    }
}
