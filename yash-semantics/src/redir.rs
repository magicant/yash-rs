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

use crate::expansion::expand_word;
use nix::fcntl::OFlag;
use std::ffi::CString;
use std::ffi::NulError;
use std::ops::Deref;
use std::ops::DerefMut;
use yash_env::expansion::Field;
use yash_env::io::Fd;
use yash_env::Env;
use yash_env::System;
use yash_syntax::source::Location;
use yash_syntax::syntax::Redir;
use yash_syntax::syntax::RedirBody;
use yash_syntax::syntax::RedirOp;

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
    /// Pathname containing a nul byte.
    NulByte(NulError),
    // TODO Other errors
}

impl From<crate::expansion::ErrorCause> for ErrorCause {
    fn from(cause: crate::expansion::ErrorCause) -> Self {
        ErrorCause::Expansion(cause)
    }
}

impl From<NulError> for ErrorCause {
    fn from(e: NulError) -> Self {
        ErrorCause::NulByte(e)
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

/// Opens a file for redirection.
fn open_file<S: System>(
    system: &mut S,
    option: OFlag,
    path: Field,
) -> Result<(Fd, Location), Error> {
    let Field { value, origin } = path;
    let path = match CString::new(value) {
        Ok(path) => path,
        Err(e) => {
            return Err(Error {
                cause: ErrorCause::NulByte(e),
                location: origin,
            })
        }
    };

    use nix::sys::stat::Mode;
    let mode = Mode::S_IRUSR
        | Mode::S_IWUSR
        | Mode::S_IRGRP
        | Mode::S_IWGRP
        | Mode::S_IROTH
        | Mode::S_IWOTH;

    match system.open(&path, option, mode) {
        Ok(fd) => Ok((fd, origin)),
        Err(e) => todo!("error handling not implemented: {:?}", e),
    }
}

/// Opens the file for a normal redirection.
async fn open_normal(
    env: &mut Env,
    operator: RedirOp,
    operand: Field,
) -> Result<(Fd, Location), Error> {
    use RedirOp::*;
    match operator {
        FileIn => open_file(&mut env.system, OFlag::O_RDONLY, operand),
        _ => todo!(),
    }
}

/// Performs a redirection.
async fn perform_one(env: &mut Env, redir: &Redir) -> Result<SavedFd, Error> {
    let (fd, _location) = match &redir.body {
        RedirBody::Normal { operator, operand } => {
            // TODO perform pathname expansion if applicable
            let expansion = expand_word(env, operand).await?;
            open_normal(env, *operator, expansion).await
        }
        RedirBody::HereDoc(_) => todo!(),
    }?;

    assert_eq!(
        fd,
        redir.fd_or_default(),
        "moving the FD is not yet supported"
    );
    // TODO Save original FD
    // TODO Move FD (dup2)

    Ok(SavedFd {
        original: fd,
        save: None,
    })
}

/// Performs redirections.
pub async fn perform<'e, 'r>(env: &'e mut Env, redirs: &'r [Redir]) -> Result<RedirEnv<'e>, Error> {
    let saved_fds = Vec::with_capacity(redirs.len());
    let mut env = RedirEnv { env, saved_fds };
    for redir in redirs {
        let saved_fd = perform_one(&mut env, redir).await?;
        env.saved_fds.push(saved_fd);
    }
    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_executor::block_on;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use yash_env::virtual_system::INode;
    use yash_env::VirtualSystem;

    #[test]
    fn basic_file_in_redirection() {
        let mut file = INode::new();
        file.content = vec![42, 123, 254];
        let system = VirtualSystem::new();
        system
            .state
            .borrow_mut()
            .file_system
            .save(PathBuf::from("foo"), Rc::new(RefCell::new(file)));
        let mut env = Env::with_system(Box::new(system));
        let redir = "3< foo".parse().unwrap();
        let mut env = block_on(perform(&mut env, std::slice::from_ref(&redir))).unwrap();

        let mut buffer = [0; 4];
        let read_count = env.system.read(Fd(3), &mut buffer).unwrap();
        assert_eq!(read_count, 3);
        assert_eq!(buffer, [42, 123, 254, 0]);
    }

    #[test]
    fn first_saved_fd_is_undone_when_second_fails() {
        // TODO implement test case
    }
}
