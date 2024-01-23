// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Initial expansion
//!
//! The initial expansion evaluates a word to a phrase. This module defines the
//! [`Expand`] trait and its implementors that perform the expansion.

use super::phrase::Phrase;
use super::Error;
use std::fmt::Debug;
use yash_env::semantics::ExitStatus;

/// Environment in which initial expansion is performed
///
/// This sturct extends [`yash_env::Env`] with some properties.
#[derive(Debug)]
pub struct Env<'a> {
    /// Main part of the environment
    pub inner: &'a mut yash_env::Env,

    /// Exit status of the last executed command substitution
    ///
    /// This value is `None` by default.
    /// When performing a command substitution during expansion, you must set
    /// its exit status to this field.
    pub last_command_subst_exit_status: Option<ExitStatus>,

    /// Whether the expansion result will be subjected to field splitting.
    ///
    /// This flag will affect the expansion of the `$*` special parameter.
    pub will_split: bool,
}

impl<'a> Env<'a> {
    /// Creates a new `Env` instance.
    ///
    /// The `last_command_subst_exit_status` and `will_split` field are
    /// initialized to be `None` and `true`, respectively.
    pub fn new(inner: &'a mut yash_env::Env) -> Self {
        Env {
            inner,
            last_command_subst_exit_status: None,
            will_split: true,
        }
    }
}

/// Syntactic construct that can be subjected to the word expansion.
///
/// Syntactic elements like [`TextUnit`](yash_syntax::syntax::TextUnit) and
/// [`Word`](yash_syntax::syntax::Word) implement this trait to
/// [`expand`](Self::expand) themselves to a [`Phrase`].
pub trait Expand {
    /// Performs initial expansion.
    #[allow(async_fn_in_trait)] // We don't support Send
    async fn expand(&self, env: &mut Env<'_>) -> Result<Phrase, Error>;
}

mod arith;
mod command_subst;
mod param;
mod slice;
mod text;
mod tilde;
mod word;

pub use arith::ArithError;
pub use param::EmptyError;
pub use param::NonassignableError;
pub use param::ValueState;
