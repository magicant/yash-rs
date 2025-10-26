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
use crate::semantics::Field;
use std::pin::Pin;
use std::rc::Rc;

type EnvPrepHook = fn(&mut Env) -> Pin<Box<dyn Future<Output = ()>>>;

type RunFunctionResult<'a> = Pin<Box<dyn Future<Output = crate::semantics::Result> + 'a>>;

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
/// The function returns a future that resolves to a [`Result`](crate::semantics::Result)
/// indicating the outcome of the function execution.
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
    pub  for<'a> fn(
        &'a mut Env,
        Rc<Function>,
        Vec<Field>,
        Option<EnvPrepHook>,
    ) -> RunFunctionResult<'a>,
);
