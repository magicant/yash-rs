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
//! # Effect of redirections
//!
//! A [redirection](Redir) modifies its [target file
//! descriptor](Redir::fd_or_default) on the basis of its [body](RedirBody).
//!
//! If the body is `Normal`, the operand word is [expanded](crate::expansion)
//! first. Then, the [operator](RedirOp) defines the next behavior:
//!
//! - `FileIn`: Opens a file for reading, regarding the expanded field as a
//!   pathname.
//! - `FileInOut`: Likewise, opens a file for reading and writing.
//! - `FileOut`, `FileClobber`: Likewise, opens a file for writing and clears
//!   the file content.  Creates an empty regular file if the file does not
//!   exist.
//! - `FileAppend`: Likewise, opens a file for appending.
//!   Creates an empty regular file if the file does not exist.
//! - `FdIn`: Copies a file descriptor, regarding the expanded field as a
//!   non-negative decimal integer denoting a readable file descriptor to copy
//!   from. Closes the target file descriptor if the field is a single hyphen
//!   (`-`) instead.
//! - `FdOut`: Likewise, copies or closes a file descriptor, but the source file
//!   descriptor must be writable instead of readable.
//! - `Pipe`: Opens a pipe, regarding the expanded field as a
//!   non-negative decimal integer denoting a file descriptor to become the
//!   reading end of the pipe. The target file descriptor will be the writing
//!   end.
//! - `String`: Opens a readable file descriptor from which you can read the
//!   expanded field followed by a newline character.
//!
//! TODO `noclobber` option
//!
//! If the body is `HereDoc`, (TODO Elaborate).
//!
//! # Performing redirections
//!
//! To perform redirections, you need to wrap an [`Env`] in a [`RedirGuard`]
//! first. Then, you call [`RedirGuard::perform_redir`] to affect the target
//! file descriptor. When you drop the `RedirGuard`, it undoes the effect to the
//! file descriptor. See the documentation for [`RedirGuard`] for details.

use crate::expansion::expand_word;
use std::borrow::Cow;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::NulError;
use std::ops::Deref;
use std::ops::DerefMut;
use yash_env::io::Fd;
use yash_env::semantics::Field;
use yash_env::system::Errno;
use yash_env::system::Mode;
use yash_env::system::OFlag;
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
    ///
    /// The `CString` is the pathname of the file that could not be opened.
    OpenFile(CString, Errno),
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
            OpenFile(_, _) => "cannot open the file",
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
            FdNotOverwritten(_, errno) => errno.desc().into(),
            OpenFile(path, errno) => format!("{}: {}", path.to_string_lossy(), errno.desc()).into(),
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

        e.location.code.source.complement_annotations(&mut a);

        Message {
            r#type: AnnotationType::Error,
            title: e.cause.message().into(),
            annotations: a,
        }
    }
}

/// Part of the shell execution environment that provides functionalities for
/// performing redirections.
pub trait Env: crate::expansion::Env {
    fn dup(&mut self, from: Fd, to_min: Fd, cloexec: bool) -> Result<Fd, Errno>;
    fn open(&mut self, path: &CStr, option: OFlag, mode: Mode) -> Result<Fd, Errno>;
}

impl Env for yash_env::Env {
    fn dup(&mut self, from: Fd, to_min: Fd, cloexec: bool) -> Result<Fd, Errno> {
        self.system.dup(from, to_min, cloexec)
    }
    fn open(&mut self, path: &CStr, option: OFlag, mode: Mode) -> Result<Fd, Errno> {
        self.system.open(path, option, mode)
    }
}

impl<E: Env> Env for crate::expansion::ExitStatusAdapter<'_, E> {
    fn dup(&mut self, from: Fd, to_min: Fd, cloexec: bool) -> Result<Fd, Errno> {
        (**self).dup(from, to_min, cloexec)
    }
    fn open(&mut self, path: &CStr, option: OFlag, mode: Mode) -> Result<Fd, Errno> {
        (**self).open(path, option, mode)
    }
}

/// Opens a file for redirection.
fn open_file<E: Env>(env: &mut E, option: OFlag, path: Field) -> Result<(Fd, Location), Error> {
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

    let mode = Mode::S_IRUSR
        | Mode::S_IWUSR
        | Mode::S_IRGRP
        | Mode::S_IWGRP
        | Mode::S_IROTH
        | Mode::S_IWOTH;

    match env.open(&path, option, mode) {
        Ok(fd) => Ok((fd, origin)),
        Err(errno) => Err(Error {
            cause: ErrorCause::OpenFile(path, errno),
            location: origin,
        }),
    }
}

/// Opens the file for a normal redirection.
async fn open_normal<E: Env>(
    env: &mut E,
    operator: RedirOp,
    operand: Field,
) -> Result<(Fd, Location), Error> {
    use RedirOp::*;
    match operator {
        FileIn => open_file(env, OFlag::O_RDONLY, operand),
        FileOut | FileClobber => open_file(
            env,
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_TRUNC,
            operand,
        ),
        FileAppend => open_file(
            env,
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_APPEND,
            operand,
        ),
        FileInOut => open_file(env, OFlag::O_RDWR | OFlag::O_CREAT, operand),
        _ => todo!(),
    }
}

/// Performs a redirection.
async fn perform<E: Env>(env: &mut E, redir: &Redir) -> Result<SavedFd, Error> {
    // Save the current open file description at `target_fd`
    let target_fd = redir.fd_or_default();
    let save = match env.dup(target_fd, MIN_SAVE_FD, true) {
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
        match env.dup2(fd, target_fd) {
            Ok(new_fd) => assert_eq!(new_fd, target_fd),
            Err(errno) => {
                // TODO Is it ok to unconditionally close fd?
                let _: Result<_, _> = env.close(fd);

                return Err(Error {
                    cause: ErrorCause::FdNotOverwritten(target_fd, errno),
                    location,
                });
            }
        }

        // Close `fd`
        let _: Result<_, _> = env.close(fd);
    }

    let original = target_fd;
    Ok(SavedFd { original, save })
}

/// `Env` wrapper for performing redirections.
///
/// This is an RAII-style wrapper of [`Env`] in which redirections are
/// performed. A `RedirGuard` keeps track of file descriptors affected by
/// redirections so that we can restore the file descriptors to the state before
/// performing the redirections.
///
/// There are two ways to clear file descriptors saved in the `RedirGuard`.  One
/// is [`undo_redirs`](Self::undo_redirs), which restores the file descriptors
/// to the original state, and the other is
/// [`preserve_redirs`](Self::preserve_redirs), which removes the saved file
/// descriptors without restoring the state and thus makes the effect of the
/// redirections permanent.
///
/// When an instance of `RedirGuard` is dropped, `undo_redirs` is implicitly
/// called. That means you need to call `preserve_redirs` explicitly to preserve
/// the redirections' effect.
#[derive(Debug)]
pub struct RedirGuard<'e, E: Env> {
    /// Environment in which redirections are performed.
    env: &'e mut E,
    /// Records of file descriptors that have been modified by redirections.
    saved_fds: Vec<SavedFd>,
}

impl<E: Env> Deref for RedirGuard<'_, E> {
    type Target = E;
    fn deref(&self) -> &E {
        self.env
    }
}

impl<E: Env> DerefMut for RedirGuard<'_, E> {
    fn deref_mut(&mut self) -> &mut E {
        self.env
    }
}

impl<E: Env> std::ops::Drop for RedirGuard<'_, E> {
    fn drop(&mut self) {
        self.undo_redirs()
    }
}

impl<'e, E: Env> RedirGuard<'e, E> {
    /// Creates a new `RedirGuard`.
    pub fn new(env: &'e mut E) -> Self {
        let saved_fds = Vec::new();
        RedirGuard { env, saved_fds }
    }

    /// Performs a redirection.
    ///
    /// If successful, this function saves internally a backing copy of the file
    /// descriptor affected by the redirection.
    pub async fn perform_redir(&mut self, redir: &Redir) -> Result<(), Error> {
        let saved_fd = perform(&mut **self, redir).await?;
        self.saved_fds.push(saved_fd);
        Ok(())
    }

    /// Performs redirections.
    ///
    /// This is a convenience function for [performing
    /// redirection](Self::perform_redir) for each iterator item.
    ///
    /// If the redirection fails for an item, the remainders are ignored, but
    /// the effects of the preceding items are not canceled.
    pub async fn perform_redirs<'a, I>(&mut self, redirs: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = &'a Redir>,
    {
        for redir in redirs {
            self.perform_redir(redir).await?;
        }
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
                let _: Result<_, _> = self.env.dup2(save, original);
                let _: Result<_, _> = self.env.close(save);
            } else {
                let _: Result<_, _> = self.env.close(original);
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
    // `RedirGuard` is dropped by `preserve_redirs`. When the outer `RedirGuard`
    // is dropped by `undo_redirs`, the undoing will move and close the FD that
    // has been made permanent by `preserve_redirs`, which is not expected.
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_executor::block_on;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use yash_env::system::r#virtual::INode;
    use yash_env::Env;
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
        let mut env = RedirGuard::new(&mut env);
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
        let mut env = RedirGuard::new(&mut env);
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
        let mut redir_env = RedirGuard::new(&mut env);
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
        let mut redir_env = RedirGuard::new(&mut env);
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
        let mut env = RedirGuard::new(&mut env);
        let redir = "< no_such_file".parse().unwrap();
        let e = block_on(env.perform_redir(&redir)).unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::OpenFile(CString::new("no_such_file").unwrap(), Errno::ENOENT)
        );
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
        let mut env = RedirGuard::new(&mut env);
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
        let mut env = RedirGuard::new(&mut env);
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
        let mut redir_env = RedirGuard::new(&mut env);
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

    #[test]
    fn file_out_creates_empty_file() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3> foo".parse().unwrap();
        block_on(env.perform_redir(&redir)).unwrap();
        env.system.write(Fd(3), &[42, 123, 57]).unwrap();

        let state = state.borrow();
        let file = state.file_system.get("foo").unwrap().borrow();
        assert_eq!(file.content, [42, 123, 57]);
    }

    #[test]
    fn file_out_truncates_existing_file() {
        let mut file = INode::new();
        file.content = vec![42, 123, 254];
        let file = Rc::new(RefCell::new(file));
        let system = VirtualSystem::new();
        system
            .state
            .borrow_mut()
            .file_system
            .save(PathBuf::from("foo"), Rc::clone(&file));
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3> foo".parse().unwrap();
        block_on(env.perform_redir(&redir)).unwrap();
        assert_eq!(file.borrow().content, []);
    }

    #[test]
    fn file_clobber_creates_empty_file() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3>| foo".parse().unwrap();
        block_on(env.perform_redir(&redir)).unwrap();
        env.system.write(Fd(3), &[42, 123, 57]).unwrap();

        let state = state.borrow();
        let file = state.file_system.get("foo").unwrap().borrow();
        assert_eq!(file.content, [42, 123, 57]);
    }

    #[test]
    fn file_clobber_by_default_truncates_existing_file() {
        let mut file = INode::new();
        file.content = vec![42, 123, 254];
        let file = Rc::new(RefCell::new(file));
        let system = VirtualSystem::new();
        system
            .state
            .borrow_mut()
            .file_system
            .save(PathBuf::from("foo"), Rc::clone(&file));
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3>| foo".parse().unwrap();
        block_on(env.perform_redir(&redir)).unwrap();
        assert_eq!(file.borrow().content, []);
    }

    // TODO file_clobber_with_noclobber_fails_with_existing_file

    #[test]
    fn file_append_creates_empty_file() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3>> foo".parse().unwrap();
        block_on(env.perform_redir(&redir)).unwrap();
        env.system.write(Fd(3), &[42, 123, 57]).unwrap();

        let state = state.borrow();
        let file = state.file_system.get("foo").unwrap().borrow();
        assert_eq!(file.content, [42, 123, 57]);
    }

    #[test]
    fn file_append_appends_to_existing_file() {
        let mut file = INode::new();
        file.content.extend("one\n".as_bytes());
        let file = Rc::new(RefCell::new(file));
        let system = VirtualSystem::new();
        system
            .state
            .borrow_mut()
            .file_system
            .save(PathBuf::from("foo"), Rc::clone(&file));
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = ">> foo".parse().unwrap();
        block_on(env.perform_redir(&redir)).unwrap();
        env.system.write(Fd::STDOUT, "two\n".as_bytes()).unwrap();

        assert_eq!(file.borrow().content, "one\ntwo\n".as_bytes());
    }

    #[test]
    fn file_in_out_creates_empty_file() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3<> foo".parse().unwrap();
        block_on(env.perform_redir(&redir)).unwrap();
        env.system.write(Fd(3), &[230, 175, 26]).unwrap();

        let state = state.borrow();
        let file = state.file_system.get("foo").unwrap().borrow();
        assert_eq!(file.content, [230, 175, 26]);
    }

    #[test]
    fn file_in_out_leaves_existing_file_content() {
        let mut file = INode::new();
        file.content = vec![132, 79, 210];
        let system = VirtualSystem::new();
        system
            .state
            .borrow_mut()
            .file_system
            .save(PathBuf::from("foo"), Rc::new(RefCell::new(file)));
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3<> foo".parse().unwrap();
        block_on(env.perform_redir(&redir)).unwrap();

        let mut buffer = [0; 4];
        let read_count = env.system.read(Fd(3), &mut buffer).unwrap();
        assert_eq!(read_count, 3);
        assert_eq!(buffer, [132, 79, 210, 0]);
    }
}
