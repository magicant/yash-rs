// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! Command execution components
//!
//! This module provides functionality related to command execution semantics.

use crate::Env;
use crate::function::Function;
use crate::semantics::{ExitStatus, Field, Result};
use crate::system::{Errno, System};
use std::convert::Infallible;
use std::ffi::CString;
use std::pin::Pin;
use std::rc::Rc;
use thiserror::Error;

type EnvPrepHook = fn(&mut Env) -> Pin<Box<dyn Future<Output = ()>>>;

type FutureResult<'a, T = ()> = Pin<Box<dyn Future<Output = Result<T>> + 'a>>;

/// Wrapper for a function that runs a shell function
///
/// This struct declares a function type that runs a shell function.
/// It is used to inject command execution behavior into the shell environment.
/// An instance of this struct can be stored in the shell environment
/// ([`Env::any`]) and used by modules that need to run shell functions.
///
/// The wrapped function takes the following arguments:
///
/// 1. A mutable reference to the shell environment (`&'a mut Env`)
/// 2. A reference-counted pointer to the shell function to be executed (`Rc<Function>`)
/// 3. A vector of fields representing the arguments to be passed to the function (`Vec<Field>`)
///     - This should not be empty; the first element is the function name and
///       the rest are the actual arguments.
/// 4. An optional environment preparation hook
///    (`Option<fn(&mut Env) -> Pin<Box<dyn Future<Output = ()>>>>`)
///     - This hook is called after setting up the local variable context. It can inject
///       additional setup logic or modify the environment before the function is executed.
///
/// The function returns a future that resolves to a [`Result`] indicating the
/// outcome of the function execution.
///
/// The most standard implementation of this type is provided in the
/// [`yash-semantics` crate](https://crates.io/crates/yash-semantics):
///
/// ```
/// # use yash_env::semantics::command::RunFunction;
/// let mut env = yash_env::Env::new_virtual();
/// env.any.insert(Box::new(RunFunction(|env, function, fields, env_prep_hook| {
///     Box::pin(async move {
///         yash_semantics::command::simple_command::execute_function_body(
///             env, function, fields, env_prep_hook
///         ).await
///     })
/// })));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct RunFunction(
    pub for<'a> fn(&'a mut Env, Rc<Function>, Vec<Field>, Option<EnvPrepHook>) -> FutureResult<'a>,
);

/// Error returned when [replacing the current process](replace_current_process) fails
#[derive(Clone, Debug, Error)]
#[error("cannot execute external utility {path:?}: {errno}")]
pub struct ReplaceCurrentProcessError {
    /// Path of the external utility attempted to be executed
    pub path: CString,
    /// Error returned by the [`execve`](System::execve) system call
    pub errno: Errno,
}

/// Substitutes the currently executing shell process with the external utility.
///
/// This function performs the very last step of the simple command execution.
/// It disables the internal signal dispositions and calls the
/// [`execve`](System::execve) system call. If the call fails, it updates
/// `env.exit_status` and returns an error, in which case the caller should
/// print an error message and terminate the current process with the exit
/// status.
///
/// If the `execve` call fails with [`ENOEXEC`](Errno::ENOEXEC), this function
/// falls back on invoking the shell with the given arguments, so that the shell
/// can interpret the script. The path to the shell executable is taken from
/// [`System::shell_path`].
///
/// If the `execve` call succeeds, the future returned by this function never
/// resolves.
///
/// This function is for implementing the simple command execution semantics and
/// the `exec` built-in utility.
pub async fn replace_current_process(
    env: &mut Env,
    path: CString,
    args: Vec<Field>,
) -> std::result::Result<Infallible, ReplaceCurrentProcessError> {
    env.traps
        .disable_internal_dispositions(&mut env.system)
        .ok();

    let args = to_c_strings(args);
    let envs = env.variables.env_c_strings();
    let Err(errno) = env.system.execve(path.as_c_str(), &args, &envs).await;
    env.exit_status = match errno {
        Errno::ENOEXEC => {
            fall_back_on_sh(&mut env.system, path.clone(), args, envs).await;
            ExitStatus::NOEXEC
        }
        Errno::ENOENT | Errno::ENOTDIR => ExitStatus::NOT_FOUND,
        _ => ExitStatus::NOEXEC,
    };
    Err(ReplaceCurrentProcessError { path, errno })
}

/// Converts fields to C strings.
fn to_c_strings(s: Vec<Field>) -> Vec<CString> {
    s.into_iter()
        .filter_map(|f| {
            let bytes = f.value.into_bytes();
            // TODO Handle interior null bytes more gracefully
            CString::new(bytes).ok()
        })
        .collect()
}

/// Invokes the shell with the given arguments.
async fn fall_back_on_sh<S: System>(
    system: &mut S,
    mut script_path: CString,
    mut args: Vec<CString>,
    envs: Vec<CString>,
) {
    // Prevent the path to be regarded as an option
    if script_path.as_bytes().starts_with("-".as_bytes()) {
        let mut bytes = script_path.into_bytes();
        bytes.splice(0..0, "./".bytes());
        script_path = CString::new(bytes).unwrap();
    }

    args.insert(1, script_path);

    // Some shells change their behavior depending on args[0].
    // We set it to "sh" for the maximum portability.
    c"sh".clone_into(&mut args[0]);

    let sh_path = system.shell_path();
    system.execve(&sh_path, &args, &envs).await.ok();
}
