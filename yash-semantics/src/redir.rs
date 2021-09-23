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
use nix::errno::Errno;
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
        for SavedFd { original, save } in self.saved_fds.drain(..).rev() {
            if let Some(save) = save {
                assert_ne!(save, original);
                let _: Result<_, _> = self.env.system.dup2(save, original);
                let _: Result<_, _> = self.env.system.close(save);
            } else {
                let _: Result<_, _> = self.env.system.close(original);
            }
        }
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
    // TODO Just closing save FDs in `preserve_redirs` would render incorrect
    // behavior in some situations. Assume `perform` is called twice, the
    // second redirection's target FD is the first's save FD, and the inner
    // `RedirEnv` is dropped by `preserve_redirs`. When the outer `RedirEnv` is
    // dropped by `undo_redirs`, the undoing will move and close the FD that has
    // been made permanent by `preserve_redirs`, which is not expected.
}

/// Types of errors that may occur in the redirection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorCause {
    /// Expansion error.
    Expansion(crate::expansion::ErrorCause),
    /// Pathname containing a nul byte.
    NulByte(NulError),
    /// The target file descriptor could not be modified for the redirection.
    FdNotOverwritten(Fd, Errno),
    /// Error while opening a file.
    OpenFile(Errno),
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
        Err(errno) => Err(Error {
            cause: ErrorCause::OpenFile(errno),
            location: origin,
        }),
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
    // Save the current open file description at `target_fd`
    let target_fd = redir.fd_or_default();
    let save = match env.system.dup(target_fd, Fd(10), true) {
        Ok(save_fd) => Some(save_fd),
        Err(Errno::EBADF) => None,
        Err(errno) => {
            return Err(Error {
                cause: ErrorCause::FdNotOverwritten(target_fd, errno),
                location: redir.body.operand().location.clone(),
            })
        }
    };

    // Prepare an FD from the redirection body
    let (fd, location) = match &redir.body {
        RedirBody::Normal { operator, operand } => {
            // TODO perform pathname expansion if applicable
            let expansion = expand_word(env, operand).await?;
            open_normal(env, *operator, expansion).await
        }
        RedirBody::HereDoc(_) => todo!(),
    }?;

    if fd != target_fd {
        // Copy the new open file description from `fd` to `target_fd`
        match env.system.dup2(fd, target_fd) {
            Ok(new_fd) => assert_eq!(new_fd, target_fd),
            Err(errno) => {
                // TODO Is it ok to unconditionally close fd?
                let _: Result<_, _> = env.system.close(fd);

                return Err(Error {
                    cause: ErrorCause::FdNotOverwritten(target_fd, errno),
                    location,
                });
            }
        }

        // Close `fd`
        let _: Result<_, _> = env.system.close(fd);
    }

    let original = target_fd;
    Ok(SavedFd { original, save })
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
    fn moving_fd() {
        let mut file = INode::new();
        file.content = vec![42, 123, 254];
        let system = VirtualSystem::new();
        system
            .state
            .borrow_mut()
            .file_system
            .save(PathBuf::from("foo"), Rc::new(RefCell::new(file)));
        let mut env = Env::with_system(Box::new(system));
        let redir = "< foo".parse().unwrap();
        let mut env = block_on(perform(&mut env, std::slice::from_ref(&redir))).unwrap();

        let mut buffer = [0; 4];
        let read_count = env.system.read(Fd::STDIN, &mut buffer).unwrap();
        assert_eq!(read_count, 3);
        assert_eq!(buffer, [42, 123, 254, 0]);

        let e = env.system.read(Fd(3), &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn saving_and_undoing_fd() {
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();
        state
            .file_system
            .save(PathBuf::from("file"), Rc::new(RefCell::new(INode::new())));
        state
            .file_system
            .get("/dev/stdin")
            .unwrap()
            .borrow_mut()
            .content
            .push(17);
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let redir = "< file".parse().unwrap();
        let redir_env = block_on(perform(&mut env, std::slice::from_ref(&redir))).unwrap();
        RedirEnv::undo_redirs(redir_env);

        let mut buffer = [0; 2];
        let read_count = env.system.read(Fd::STDIN, &mut buffer).unwrap();
        assert_eq!(read_count, 1);
        assert_eq!(buffer[0], 17);
    }

    #[test]
    fn undoing_without_initial_fd() {
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();
        state
            .file_system
            .save(PathBuf::from("input"), Rc::new(RefCell::new(INode::new())));
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let redir = "4< input".parse().unwrap();
        let redir_env = block_on(perform(&mut env, std::slice::from_ref(&redir))).unwrap();
        RedirEnv::undo_redirs(redir_env);

        let mut buffer = [0; 1];
        let e = env.system.read(Fd(4), &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn unreadable_file() {
        let mut env = Env::new_virtual();
        let redir = "< no_such_file".parse().unwrap();
        let e = block_on(perform(&mut env, std::slice::from_ref(&redir))).unwrap_err();
        assert_eq!(e.cause, ErrorCause::OpenFile(Errno::ENOENT));
        assert_eq!(e.location, redir.body.operand().location);
    }

    #[test]
    fn multiple_redirections() {
        // TODO implement test case
    }

    #[test]
    fn later_redirection_wins() {
        // TODO implement test case
    }

    #[test]
    fn first_saved_fd_is_undone_when_second_fails() {
        // TODO implement test case
    }
}
