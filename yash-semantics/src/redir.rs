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
//! To perform redirections, you need to wrap an [`Env`] in a [`RedirEnv`]
//! first. Then, you call [`RedirEnv::perform_redir`] to affect the target file
//! descriptor. When you drop the `RedirEnv`, it undoes the effect to the file
//! descriptor. See the documentation for [`RedirEnv`] for details.

use crate::expansion::expand_word;
use nix::errno::Errno;
use nix::fcntl::OFlag;
use std::borrow::Cow;
use std::ffi::CString;
use std::ffi::NulError;
use std::ops::Deref;
use std::ops::DerefMut;
use yash_env::expansion::Field;
use yash_env::io::Fd;
use yash_env::Env;
use yash_env::System;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;
use yash_syntax::syntax::Redir;
use yash_syntax::syntax::RedirBody;
use yash_syntax::syntax::RedirOp;

const MIN_SAVE_FD: Fd = Fd(10);

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

impl ErrorCause {
    /// Returns an error message describing the error.
    #[must_use]
    pub fn message(&self) -> &str {
        // TODO Localize
        use ErrorCause::*;
        match self {
            Expansion(e) => e.message(),
            NulByte(_) => "nul byte found in the pathname",
            FdNotOverwritten(_, _) => "cannot redirect the file descriptor",
            OpenFile(_) => "cannot open the file",
        }
    }

    /// Returns a label for annotating the error location.
    #[must_use]
    pub fn label(&self) -> Cow<'_, str> {
        // TODO Localize
        use ErrorCause::*;
        match self {
            Expansion(e) => e.label(),
            NulByte(_) => "pathname should not contain a nul byte".into(),
            FdNotOverwritten(_, errno) | OpenFile(errno) => errno.desc().into(),
        }
    }
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

impl<'a> From<&'a Error> for Message<'a> {
    fn from(e: &'a Error) -> Self {
        let mut a = vec![Annotation {
            r#type: AnnotationType::Error,
            label: e.cause.label(),
            location: e.location.clone(),
        }];

        e.location.line.source.complement_annotations(&mut a);

        Message {
            r#type: AnnotationType::Error,
            title: e.cause.message().into(),
            annotations: a,
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
async fn perform(env: &mut Env, redir: &Redir) -> Result<SavedFd, Error> {
    // Save the current open file description at `target_fd`
    let target_fd = redir.fd_or_default();
    let save = match env.system.dup(target_fd, MIN_SAVE_FD, true) {
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

/// `Env` wrapper for performing redirections.
///
/// This is an RAII-style wrapper of [`Env`] in which redirections are
/// performed. A `RedirEnv` keeps track of file descriptors affected by
/// redirections so that we can restore the file descriptors to the state before
/// performing the redirections.
///
/// There are two ways to clear file descriptors saved in the `RedirEnv`.
/// One is [`undo_redirs`](Self::undo_redirs), which restores the file
/// descriptors to the original state, and the other is
/// [`preserve_redirs`](Self::preserve_redirs), which removes the saved file
/// descriptors without restoring the state and thus makes the effect of the
/// redirections permanent.
///
/// When an instance of `RedirEnv` is dropped, `undo_redirs` is implicitly
/// called. That means you need to call `preserve_redirs` explicitly to preserve
/// the redirections' effect.
#[derive(Debug)]
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
        self.undo_redirs()
    }
}

impl RedirEnv<'_> {
    /// Creates a new `RedirEnv`.
    pub fn new(env: &mut Env) -> RedirEnv<'_> {
        let saved_fds = Vec::new();
        RedirEnv { env, saved_fds }
    }

    /// Performs a redirection.
    ///
    /// If successful, this function saves internally a backing copy of the file
    /// descriptor affected by the redirection.
    pub async fn perform_redir(&mut self, redir: &Redir) -> Result<(), Error> {
        let saved_fd = perform(self, redir).await?;
        self.saved_fds.push(saved_fd);
        Ok(())
    }

    /// Undoes the effect of the redirections.
    ///
    /// This function restores the file descriptors affected by redirections to
    /// the original state and closes internal backing file descriptors, which
    /// were used for restoration and are no longer needed.
    pub fn undo_redirs(&mut self) {
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

    /// Makes the redirections permanent.
    ///
    /// This function closes internal backing file descriptors without restoring
    /// the original file descriptor state.
    pub fn preserve_redirs(&mut self) {
        todo!()
    }
    // TODO Just closing save FDs in `preserve_redirs` would render incorrect
    // behavior in some situations. Assume `perform` is called twice, the
    // second redirection's target FD is the first's save FD, and the inner
    // `RedirEnv` is dropped by `preserve_redirs`. When the outer `RedirEnv` is
    // dropped by `undo_redirs`, the undoing will move and close the FD that has
    // been made permanent by `preserve_redirs`, which is not expected.
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
        let mut env = RedirEnv::new(&mut env);
        let redir = "3< foo".parse().unwrap();
        block_on(env.perform_redir(&redir)).unwrap();

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
        let mut env = RedirEnv::new(&mut env);
        let redir = "< foo".parse().unwrap();
        block_on(env.perform_redir(&redir)).unwrap();

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
        let mut redir_env = RedirEnv::new(&mut env);
        let redir = "< file".parse().unwrap();
        block_on(redir_env.perform_redir(&redir)).unwrap();
        redir_env.undo_redirs();
        drop(redir_env);

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
        let mut redir_env = RedirEnv::new(&mut env);
        let redir = "4< input".parse().unwrap();
        block_on(redir_env.perform_redir(&redir)).unwrap();
        redir_env.undo_redirs();
        drop(redir_env);

        let mut buffer = [0; 1];
        let e = env.system.read(Fd(4), &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn unreadable_file() {
        let mut env = Env::new_virtual();
        let mut env = RedirEnv::new(&mut env);
        let redir = "< no_such_file".parse().unwrap();
        let e = block_on(env.perform_redir(&redir)).unwrap_err();
        assert_eq!(e.cause, ErrorCause::OpenFile(Errno::ENOENT));
        assert_eq!(e.location, redir.body.operand().location);
    }

    #[test]
    fn multiple_redirections() {
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();

        let mut file = INode::new();
        file.content = vec![100];
        state
            .file_system
            .save(PathBuf::from("foo"), Rc::new(RefCell::new(file)));

        let mut file = INode::new();
        file.content = vec![200];
        state
            .file_system
            .save(PathBuf::from("bar"), Rc::new(RefCell::new(file)));

        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirEnv::new(&mut env);
        block_on(env.perform_redir(&"< foo".parse().unwrap())).unwrap();
        block_on(env.perform_redir(&"3< bar".parse().unwrap())).unwrap();

        let mut buffer = [0; 1];
        let read_count = env.system.read(Fd::STDIN, &mut buffer).unwrap();
        assert_eq!(read_count, 1);
        assert_eq!(buffer, [100]);
        let read_count = env.system.read(Fd(3), &mut buffer).unwrap();
        assert_eq!(read_count, 1);
        assert_eq!(buffer, [200]);
    }

    #[test]
    fn later_redirection_wins() {
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();

        let mut file = INode::new();
        file.content = vec![100];
        state
            .file_system
            .save(PathBuf::from("foo"), Rc::new(RefCell::new(file)));

        let mut file = INode::new();
        file.content = vec![200];
        state
            .file_system
            .save(PathBuf::from("bar"), Rc::new(RefCell::new(file)));

        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirEnv::new(&mut env);
        block_on(env.perform_redir(&"< foo".parse().unwrap())).unwrap();
        block_on(env.perform_redir(&"< bar".parse().unwrap())).unwrap();

        let mut buffer = [0; 1];
        let read_count = env.system.read(Fd::STDIN, &mut buffer).unwrap();
        assert_eq!(read_count, 1);
        assert_eq!(buffer, [200]);
    }

    #[test]
    fn undo_save_conflict() {
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();

        let mut file = INode::new();
        file.content = vec![10];
        state
            .file_system
            .save(PathBuf::from("foo"), Rc::new(RefCell::new(file)));

        let mut file = INode::new();
        file.content = vec![20];
        state
            .file_system
            .save(PathBuf::from("bar"), Rc::new(RefCell::new(file)));

        state
            .file_system
            .get("/dev/stdin")
            .unwrap()
            .borrow_mut()
            .content
            .push(30);

        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut redir_env = RedirEnv::new(&mut env);
        block_on(redir_env.perform_redir(&"< foo".parse().unwrap())).unwrap();
        block_on(redir_env.perform_redir(&"10< bar".parse().unwrap())).unwrap();
        drop(redir_env);

        let mut buffer = [0; 1];
        let e = env.system.read(MIN_SAVE_FD, &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
        let read_count = env.system.read(Fd::STDIN, &mut buffer).unwrap();
        assert_eq!(read_count, 1);
        assert_eq!(buffer, [30]);
    }
}
