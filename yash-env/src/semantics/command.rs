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
//! This module defines types for injecting command execution behavior.

use crate::Env;
use crate::function::Function;
use crate::semantics::{ExitStatus, Field, Result};
use std::ffi::CString;
use std::pin::Pin;
use std::rc::Rc;

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

/// Wrapper for running an external utility in a subshell
///
/// This struct declares a function type that runs an external utility in a
/// subshell. It is used to inject command execution behavior into the shell
/// environment. An instance of this struct can be stored in the shell
/// environment ([`Env::any`]) and used by modules that need to run external
/// utilities in a subshell.
///
/// The wrapped function takes three arguments:
///
/// 1. A mutable reference to the shell environment (`&'a mut Env`)
/// 2. A `CString` representing the path to the external utility to be executed
/// 3. A vector of [`Field`]s representing the arguments to be passed to the utility
///
/// The vector must not be empty; the first element is the utility name and may
/// be used for error messages.
///
/// Errors creating the subshell or executing the utility should be handled in
/// the wrapped function itself, and the function should return a future that
/// resolves to an [`ExitStatus`] indicating the outcome of the execution.
///
/// The most standard implementation of this type is provided in the
/// [`yash-semantics` crate](https://crates.io/crates/yash-semantics):
///
/// ```
/// # use yash_env::semantics::command::RunExternalUtilityInSubshell;
/// let mut env = yash_env::Env::new_virtual();
/// env.any.insert(Box::new(RunExternalUtilityInSubshell(
///     |env, path, fields| Box::pin(async move {
///         yash_semantics::command::simple_command::start_external_utility_in_subshell_and_wait(
///             env, path, fields,
///         )
///         .await
///     }),
/// )));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct RunExternalUtilityInSubshell(
    pub for<'a> fn(&'a mut Env, CString, Vec<Field>) -> FutureResult<'a, ExitStatus>,
);
