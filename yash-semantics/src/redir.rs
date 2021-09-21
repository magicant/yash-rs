// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Redirection semantics.
//!
//! TODO Elaborate

use std::ops::Deref;
use std::ops::DerefMut;
use yash_env::io::Fd;
use yash_env::Env;
use yash_syntax::source::Location;
use yash_syntax::syntax::Redir;

/// Record of saving an open file description in another file descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SavedFd {
    /// File descriptor by which the original open file description was
    /// previously accessible.
    original: Fd,
    /// Temporary file descriptor that remembers the original open file
    /// description.
    save: Option<Fd>,
}

/// Environment (temporarily) modified by redirections.
///
/// This is an RAII-style guard that cancels the effect of redirections when the
/// instance is dropped. The [`perform`] function returns an instance of
/// `RedirEnv`. When you drop the instance, the file descriptors modified by the
/// redirections are restored to the original state.
///
/// Instead of restoring the state, you can optionally preserve the effect of
/// redirections permanently even after the instance is dropped. To do so, call
/// the [`preserve_redirs`](Self::preserve_redirs) function.
#[derive(Debug)]
#[must_use = "Redirections are cancelled when you drop RedirEnv"]
pub struct RedirEnv<'e> {
    /// Environment in which redirections are performed.
    env: &'e mut Env,
    /// Records of file descriptors that have been modified by redirections.
    saved_fds: Vec<SavedFd>,
}

impl Deref for RedirEnv<'_> {
    type Target = Env;
    fn deref(&self) -> &Env {
        self.env
    }
}

impl DerefMut for RedirEnv<'_> {
    fn deref_mut(&mut self) -> &mut Env {
        self.env
    }
}

impl std::ops::Drop for RedirEnv<'_> {
    fn drop(&mut self) {
        // TODO undo redirections
    }
}

impl RedirEnv<'_> {
    /// Undo the effect of the redirections.
    pub fn undo_redirs(self) {
        drop(self)
    }

    /// Make the redirections permanent.
    ///
    /// This function drops the `RedirEnv` without undoing the effect of the
    /// redirections.
    pub fn preserve_redirs(self) {
        todo!()
    }
}

/// Types of errors that may occur in the redirection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorCause {
    /// Expansion error.
    Expansion(crate::expansion::ErrorCause),
    // TODO Other errors
}

impl From<crate::expansion::ErrorCause> for ErrorCause {
    fn from(cause: crate::expansion::ErrorCause) -> Self {
        ErrorCause::Expansion(cause)
    }
}

/// Explanation of a redirection error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub cause: ErrorCause,
    pub location: Location,
}

impl From<crate::expansion::Error> for Error {
    fn from(e: crate::expansion::Error) -> Self {
        Error {
            cause: e.cause.into(),
            location: e.location,
        }
    }
}

/// Performs redirections.
pub async fn perform<'e, 'r>(
    env: &'e mut Env,
    _redirs: &'r [Redir],
) -> Result<RedirEnv<'e>, Error> {
    Ok(RedirEnv {
        env,
        saved_fds: vec![],
    })
    /*
    TODO
    1. Expand filename
    2. Open FD (open file, prepare here-document content, open pipe)
    3. Save original FD
    4. Move FD (dup2)
        */
}
