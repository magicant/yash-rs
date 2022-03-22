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
//! TODO Elaborate

use super::phrase::Phrase;
use super::Error;
use async_trait::async_trait;
use std::ops::Deref;
use std::ops::DerefMut;
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

    /// Whether the currently expanded part is double-quoted
    ///
    /// This field affects the behavior of some expansions.
    ///
    /// Rather than modifying this flag manually, you should call
    /// [`begin_quote`](Self::begin_quote) and use [`QuoteGuard`] to ensure the
    /// flag is cleared when exiting the quote.
    pub is_quoted: bool,
}

impl<'a> Env<'a> {
    /// Creates a new `Env` instance.
    pub fn new(inner: &'a mut yash_env::Env) -> Self {
        Env {
            inner,
            last_command_subst_exit_status: None,
            is_quoted: false,
        }
    }

    /// Sets the `is_quoted` flag to `true` and returns a guard that will
    /// restore it.
    ///
    /// Functions that expand a double-quote must call this function before
    /// expanding the contents of the quote.
    pub fn begin_quote<'b>(&'b mut self) -> QuoteGuard<'b, 'a> {
        let was_quoted = self.is_quoted;
        self.is_quoted = true;
        QuoteGuard {
            env: self,
            was_quoted,
        }
    }

    /// Restores the `Env::is_quoted` flag to the original value.
    pub fn end_quote(guard: QuoteGuard<'_, '_>) {
        drop(guard)
    }
}

/// RAII-style guard for restoring the value of `Env::is_quoted`
///
/// [`Env::begin_quote`] returns a `QuoteGuard` object after setting the
/// [`Env::is_quoted`] flag to `true`. When the guard is dropped or passed to
/// [`Env::end_quote`], the flag is restored to the value before beginning the
/// quote.
///
/// This struct implements `Deref` and `DerefMut` so you can access the original
/// `Env` by dereferencing it.
#[must_use]
pub struct QuoteGuard<'a, 'b> {
    env: &'a mut Env<'b>,
    was_quoted: bool,
}

impl Drop for QuoteGuard<'_, '_> {
    fn drop(&mut self) {
        self.env.is_quoted = self.was_quoted;
    }
}

impl<'a> Deref for QuoteGuard<'_, 'a> {
    type Target = Env<'a>;
    fn deref(&self) -> &Env<'a> {
        self.env
    }
}

impl<'a> DerefMut for QuoteGuard<'_, 'a> {
    fn deref_mut(&mut self) -> &mut Env<'a> {
        self.env
    }
}

/// Return value of [`Expand::quick_expand`].
pub enum QuickExpand<T: Expand + ?Sized> {
    /// Variant returned if the expansion is complete.
    Ready(Result<Phrase, Error>),
    /// Variant returned if the expansion needs to be resumed.
    Interim(<T as Expand>::Interim),
}

impl<T: Expand + ?Sized> std::fmt::Debug for QuickExpand<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuickExpand::Ready(result) => f.debug_tuple("Ready").field(result).finish(),
            QuickExpand::Interim(_) => f.debug_struct("Interim").finish_non_exhaustive(),
        }
    }
}

/// Syntactic construct that can be subjected to the word expansion.
///
/// Syntactic elements like [`TextUnit`](yash_syntax::syntax::TextUnit) and
/// [`Word`](yash_syntax::syntax::Word) implement this trait to expand
/// themselves to a [`Phrase`].
///
/// This trait defines two functions. You first call
/// [`quick_expand`](Self::quick_expand) to start the expansion. It immediately
/// returns the result as `QuickExpand::Ready(_)` if it completes without any
/// asynchronous operation (either successfully or in failure). If the expansion
/// requires an asynchronous computation, `quick_expand` returns
/// `QuickExpand::Interim(_)` containing interim data. You continue the
/// expansion by passing the data to [`async_expand`](Self::async_expand), which
/// produces a future that will yield the final result. This two-step procedure
/// works around a limitation imposed by the current Rust compiler
/// (cf. [#68117](https://github.com/rust-lang/rust/issues/68117)).
#[async_trait(?Send)]
pub trait Expand {
    /// Data passed from [`quick_expand`](Self::quick_expand) to
    /// [`async_expand`](Self::async_expand).
    type Interim;

    /// Starts the initial expansion.
    ///
    /// If the expansion completes without an asynchronous operation, the result
    /// is returned in `QuickExpand::Ready(_)`. Otherwise, the result is
    /// `QuickExpand::Interim(_)` containing interim data that should be passed
    /// to [`async_expand`](Self::async_expand).
    fn quick_expand(&self, env: &mut Env<'_>) -> QuickExpand<Self>;

    /// Continues the initial expansion.
    ///
    /// You should call this function if [`quick_expand`](Self::quick_expand)
    /// returns `Err(interim)`. This function returns a boxed future that will
    /// produce a final result.
    async fn async_expand(
        &self,
        env: &mut Env<'_>,
        interim: Self::Interim,
    ) -> Result<Phrase, Error>;
}

mod command_subst;
mod slice;
mod text;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reentrant_quotes() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        assert!(!env.is_quoted);
        {
            let mut first = env.begin_quote();
            assert!(first.is_quoted);
            let second = first.begin_quote();
            assert!(second.is_quoted);
            Env::end_quote(second);
            assert!(first.is_quoted);
        }
        assert!(!env.is_quoted);
    }
}
