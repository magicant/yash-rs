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

//! Types for injecting prompt string retrieval functions
//!
//! This module defines the type used for dependency injection:
//! a function that obtains the prompt string from the environment.
//!
//! The concrete implementation of that function is not provided by this crate;
//! it is implemented in another crate and stored inside [`Env::any`]. Callers
//! that need a prompt (for example the interactive input loop or the `read`
//! built-in) retrieve the injected function from `Env::any` and invoke it to
//! produce prompt strings.
//!
//! This file only supplies the type used for injection; registration and
//! concrete implementations are the responsibility of other crates that set
//! up the environment.

use crate::Env;
use crate::input::Context;
use std::pin::Pin;

type PinFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Wrapper around the function that retrieves a prompt string
///
/// The wrapped function is expected to be injected into the environment (via
/// [`Env::any`]) by another crate and may be referenced by multiple consumers.
///
/// The function takes two arguments: the environment and the input context. It
/// returns a future that resolves to the final prompt string to be displayed.
/// Any errors encountered while generating the prompt string should be handled
/// within the function itself; the function should always resolve to a valid
/// string.
///
/// The most standard way to implement such a function is to use the
/// [`yash-prompt` crate](https://crates.io/crates/yash-prompt) to fetch and
/// expand the prompt string.
///
/// ```
/// # use yash_env::{VirtualSystem, prompt::GetPrompt};
/// let mut env = yash_env::Env::new_virtual();
/// env.any.insert(Box::new(GetPrompt::<VirtualSystem>(|env, context| {
///     Box::pin(async move {
///         let prompt = yash_prompt::fetch_posix(&env.variables, &context);
///         yash_prompt::expand_posix(env, &prompt, false).await
///     })
/// })));
/// ```
#[derive(Debug)]
pub struct GetPrompt<S>(pub for<'a> fn(&'a mut Env<S>, &'a Context) -> PinFuture<'a, String>);

// Not derived automatically because S may not implement Clone or Copy.
impl<S> Clone for GetPrompt<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S> Copy for GetPrompt<S> {}
