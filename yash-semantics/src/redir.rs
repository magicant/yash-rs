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
use std::ffi::CString;
use std::ffi::NulError;
use std::num::ParseIntError;
use std::ops::Deref;
use std::ops::DerefMut;
use yash_env::io::Fd;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::system::Errno;
use yash_env::system::Mode;
use yash_env::system::OFlag;
use yash_env::Env;
use yash_env::System;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;
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
    /// Operand of `<&` or `>&` that cannot be parsed as an integer.
    MalformedFd(String, ParseIntError),
    /// `<&` applied to an unreadable file descriptor
    UnreadableFd(Fd),
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
            MalformedFd(_, _) => "not a valid file descriptor",
            UnreadableFd(_) => "cannot copy file descriptor",
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
            MalformedFd(value, error) => format!("{}: {}", value, error).into(),
            UnreadableFd(fd) => format!("{}: not a readable file descriptor", fd).into(),
        }
    }
}

impl std::fmt::Display for ErrorCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ErrorCause::*;
        match self {
            Expansion(e) => e.fmt(f),
            NulByte(error) => error.fmt(f),
            FdNotOverwritten(_fd, errno) => errno.fmt(f),
            OpenFile(path, errno) => write!(
                f,
                "cannot open file `{}`: {}",
                path.to_string_lossy(),
                errno
            ),
            MalformedFd(value, error) => {
                write!(f, "{:?} is not a valid file descriptor: {}", value, error)
            }
            UnreadableFd(fd) => write!(f, "{} is not a readable file descriptor", fd),
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

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}

impl std::error::Error for Error {}

impl From<crate::expansion::Error> for Error {
    fn from(e: crate::expansion::Error) -> Self {
        Error {
            cause: e.cause.into(),
            location: e.location,
        }
    }
}

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        self.cause.message().into()
    }

    fn main_annotation(&self) -> Annotation {
        Annotation::new(AnnotationType::Error, self.cause.label(), &self.location)
    }
}

/// Intermediate state of a redirected file descriptor
#[derive(Debug)]
enum FdSpec {
    /// File descriptor specifically opened for redirection
    Owned(Fd),
    /// Existing file descriptor
    Borrowed(Fd),
    /// Closed file descriptor
    Closed,
}

impl FdSpec {
    fn as_fd(&self) -> Option<Fd> {
        match self {
            &FdSpec::Owned(fd) | &FdSpec::Borrowed(fd) => Some(fd),
            &FdSpec::Closed => None,
        }
    }

    fn close<S: System>(self, system: &mut S) {
        match self {
            FdSpec::Owned(fd) => {
                let _ = system.close(fd);
            }
            FdSpec::Borrowed(_) | FdSpec::Closed => (),
        }
    }
}

/// Opens a file for redirection.
fn open_file(env: &mut Env, option: OFlag, path: Field) -> Result<(FdSpec, Location), Error> {
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

    match env.system.open(&path, option, mode) {
        Ok(fd) => Ok((FdSpec::Owned(fd), origin)),
        Err(errno) => Err(Error {
            cause: ErrorCause::OpenFile(path, errno),
            location: origin,
        }),
    }
}

/// Parses the target of `<&` and `>&`.
fn copy_fd(env: &mut Env, target: Field) -> Result<(FdSpec, Location), Error> {
    if target.value == "-" {
        return Ok((FdSpec::Closed, target.origin));
    }

    // Parse the string as an integer
    let fd = match target.value.parse() {
        Ok(number) => Fd(number),
        Err(error) => {
            return Err(Error {
                cause: ErrorCause::MalformedFd(target.value, error),
                location: target.origin,
            })
        }
    };

    // Check if the FD is really readable or writable
    if let Ok(flags) = env.system.fcntl_getfl(fd) {
        let mode = flags & OFlag::O_ACCMODE;
        if !(mode == OFlag::O_RDONLY || mode == OFlag::O_RDWR) {
            return Err(Error {
                cause: ErrorCause::UnreadableFd(fd),
                location: target.origin,
            });
        }
        // TODO Check if the FD is really writable
    }

    Ok((FdSpec::Borrowed(fd), target.origin))
}

/// Opens the file for a normal redirection.
async fn open_normal(
    env: &mut Env,
    operator: RedirOp,
    operand: Field,
) -> Result<(FdSpec, Location), Error> {
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
        FdIn | FdOut => copy_fd(env, operand),
        _ => todo!(),
    }
}

/// Performs a redirection.
async fn perform(env: &mut Env, redir: &Redir) -> Result<(SavedFd, Option<ExitStatus>), Error> {
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
    let (fd_spec, location, exit_status) = match &redir.body {
        RedirBody::Normal { operator, operand } => {
            // TODO perform pathname expansion if applicable
            let (expansion, exit_status) = expand_word(env, operand).await?;
            let (fd, location) = open_normal(env, *operator, expansion).await?;
            (fd, location, exit_status)
        }
        RedirBody::HereDoc(_) => todo!(),
    };

    if let Some(fd) = fd_spec.as_fd() {
        if fd != target_fd {
            let dup_result = env.system.dup2(fd, target_fd);
            fd_spec.close(&mut env.system);
            match dup_result {
                Ok(new_fd) => assert_eq!(new_fd, target_fd),
                Err(errno) => {
                    return Err(Error {
                        cause: ErrorCause::FdNotOverwritten(target_fd, errno),
                        location,
                    });
                }
            }
        }
    } else {
        let _: Result<(), Errno> = env.system.close(target_fd);
    }

    let original = target_fd;
    Ok((SavedFd { original, save }, exit_status))
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
pub struct RedirGuard<'e> {
    /// Environment in which redirections are performed.
    env: &'e mut yash_env::Env,
    /// Records of file descriptors that have been modified by redirections.
    saved_fds: Vec<SavedFd>,
}

impl Deref for RedirGuard<'_> {
    type Target = yash_env::Env;
    fn deref(&self) -> &yash_env::Env {
        self.env
    }
}

impl DerefMut for RedirGuard<'_> {
    fn deref_mut(&mut self) -> &mut yash_env::Env {
        self.env
    }
}

impl std::ops::Drop for RedirGuard<'_> {
    fn drop(&mut self) {
        self.undo_redirs()
    }
}

impl<'e> RedirGuard<'e> {
    /// Creates a new `RedirGuard`.
    pub fn new(env: &'e mut yash_env::Env) -> Self {
        let saved_fds = Vec::new();
        RedirGuard { env, saved_fds }
    }

    /// Performs a redirection.
    ///
    /// If successful, this function saves internally a backing copy of the file
    /// descriptor affected by the redirection, and returns the exit status of
    /// the last command substitution performed during the redirection, if any.
    pub async fn perform_redir(&mut self, redir: &Redir) -> Result<Option<ExitStatus>, Error> {
        let (saved_fd, exit_status) = perform(self, redir).await?;
        self.saved_fds.push(saved_fd);
        Ok(exit_status)
    }

    /// Performs redirections.
    ///
    /// This is a convenience function for [performing
    /// redirection](Self::perform_redir) for each iterator item.
    ///
    /// If the redirection fails for an item, the remainders are ignored, but
    /// the effects of the preceding items are not canceled.
    pub async fn perform_redirs<'a, I>(&mut self, redirs: I) -> Result<Option<ExitStatus>, Error>
    where
        I: IntoIterator<Item = &'a Redir>,
    {
        let mut exit_status = None;
        for redir in redirs {
            let new_exit_status = self.perform_redir(redir).await?;
            exit_status = new_exit_status.or(exit_status);
        }
        Ok(exit_status)
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
    // `RedirGuard` is dropped by `preserve_redirs`. When the outer `RedirGuard`
    // is dropped by `undo_redirs`, the undoing will move and close the FD that
    // has been made permanent by `preserve_redirs`, which is not expected.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use std::rc::Rc;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::r#virtual::INode;
    use yash_env::Env;
    use yash_env::VirtualSystem;

    #[test]
    fn basic_file_in_redirection() {
        let system = VirtualSystem::new();
        let file = Rc::new(RefCell::new(INode::new([42, 123, 254])));
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", file).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3< foo".parse().unwrap();
        let result = env.perform_redir(&redir).now_or_never().unwrap().unwrap();
        assert_eq!(result, None);

        let mut buffer = [0; 4];
        let read_count = env.system.read(Fd(3), &mut buffer).unwrap();
        assert_eq!(read_count, 3);
        assert_eq!(buffer, [42, 123, 254, 0]);
    }

    #[test]
    fn moving_fd() {
        let system = VirtualSystem::new();
        let file = Rc::new(RefCell::new(INode::new([42, 123, 254])));
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", file).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "< foo".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();

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
        state.file_system.save("file", Rc::default()).unwrap();
        state
            .file_system
            .get("/dev/stdin")
            .unwrap()
            .borrow_mut()
            .body = FileBody::new([17]);
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut redir_env = RedirGuard::new(&mut env);
        let redir = "< file".parse().unwrap();
        redir_env
            .perform_redir(&redir)
            .now_or_never()
            .unwrap()
            .unwrap();
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
        state.file_system.save("input", Rc::default()).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut redir_env = RedirGuard::new(&mut env);
        let redir = "4< input".parse().unwrap();
        redir_env
            .perform_redir(&redir)
            .now_or_never()
            .unwrap()
            .unwrap();
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
        let e = env
            .perform_redir(&redir)
            .now_or_never()
            .unwrap()
            .unwrap_err();
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
        let file = Rc::new(RefCell::new(INode::new([100])));
        state.file_system.save("foo", file).unwrap();
        let file = Rc::new(RefCell::new(INode::new([200])));
        state.file_system.save("bar", file).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        env.perform_redir(&"< foo".parse().unwrap())
            .now_or_never()
            .unwrap()
            .unwrap();
        env.perform_redir(&"3< bar".parse().unwrap())
            .now_or_never()
            .unwrap()
            .unwrap();

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
        let file = Rc::new(RefCell::new(INode::new([100])));
        state.file_system.save("foo", file).unwrap();
        let file = Rc::new(RefCell::new(INode::new([200])));
        state.file_system.save("bar", file).unwrap();
        drop(state);

        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        env.perform_redir(&"< foo".parse().unwrap())
            .now_or_never()
            .unwrap()
            .unwrap();
        env.perform_redir(&"< bar".parse().unwrap())
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut buffer = [0; 1];
        let read_count = env.system.read(Fd::STDIN, &mut buffer).unwrap();
        assert_eq!(read_count, 1);
        assert_eq!(buffer, [200]);
    }

    #[test]
    fn undo_save_conflict() {
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();
        let file = Rc::new(RefCell::new(INode::new([10])));
        state.file_system.save("foo", file).unwrap();
        let file = Rc::new(RefCell::new(INode::new([20])));
        state.file_system.save("bar", file).unwrap();
        state
            .file_system
            .get("/dev/stdin")
            .unwrap()
            .borrow_mut()
            .body = FileBody::new([30]);
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut redir_env = RedirGuard::new(&mut env);
        redir_env
            .perform_redir(&"< foo".parse().unwrap())
            .now_or_never()
            .unwrap()
            .unwrap();
        redir_env
            .perform_redir(&"10< bar".parse().unwrap())
            .now_or_never()
            .unwrap()
            .unwrap();
        drop(redir_env);

        let mut buffer = [0; 1];
        let e = env.system.read(MIN_SAVE_FD, &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
        let read_count = env.system.read(Fd::STDIN, &mut buffer).unwrap();
        assert_eq!(read_count, 1);
        assert_eq!(buffer, [30]);
    }

    #[test]
    fn exit_status_of_command_substitution() {
        in_virtual_system(|mut env, _pid, state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.builtins.insert("return", return_builtin());
            let mut env = RedirGuard::new(&mut env);
            let redir = "3> $(echo foo; return -n 79)".parse().unwrap();
            let result = env.perform_redir(&redir).await.unwrap();
            assert_eq!(result, Some(ExitStatus(79)));
            let file = state.borrow().file_system.get("foo");
            assert!(file.is_ok(), "{:?}", file);
        })
    }

    #[test]
    fn file_in_closes_opened_file_on_error() {
        let mut env = Env::new_virtual();
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999</dev/stdin".parse().unwrap();
        let e = env
            .perform_redir(&redir)
            .now_or_never()
            .unwrap()
            .unwrap_err();

        assert_eq!(
            e.cause,
            ErrorCause::FdNotOverwritten(Fd(999999999), Errno::EBADF)
        );
        assert_eq!(e.location, redir.body.operand().location);
        let mut buffer = [0; 1];
        let e = env.system.read(Fd(3), &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn file_out_creates_empty_file() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3> foo".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();
        env.system.write(Fd(3), &[42, 123, 57]).unwrap();

        let file = state.borrow().file_system.get("foo").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], [42, 123, 57]);
        });
    }

    #[test]
    fn file_out_truncates_existing_file() {
        let file = Rc::new(RefCell::new(INode::new([42, 123, 254])));
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", Rc::clone(&file)).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);

        let redir = "3> foo".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();

        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], []);
        });
    }

    #[test]
    fn file_out_closes_opened_file_on_error() {
        let mut env = Env::new_virtual();
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999>foo".parse().unwrap();
        let e = env
            .perform_redir(&redir)
            .now_or_never()
            .unwrap()
            .unwrap_err();

        assert_eq!(
            e.cause,
            ErrorCause::FdNotOverwritten(Fd(999999999), Errno::EBADF)
        );
        assert_eq!(e.location, redir.body.operand().location);
        let e = env.system.write(Fd(3), &[0x20]).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn file_clobber_creates_empty_file() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);

        let redir = "3>| foo".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();
        env.system.write(Fd(3), &[42, 123, 57]).unwrap();

        let file = state.borrow().file_system.get("foo").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], [42, 123, 57]);
        });
    }

    #[test]
    fn file_clobber_by_default_truncates_existing_file() {
        let file = Rc::new(RefCell::new(INode::new([42, 123, 254])));
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", Rc::clone(&file)).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);

        let redir = "3>| foo".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();

        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], []);
        });
    }

    // TODO file_clobber_with_noclobber_fails_with_existing_file

    #[test]
    fn file_clobber_closes_opened_file_on_error() {
        let mut env = Env::new_virtual();
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999>|foo".parse().unwrap();
        let e = env
            .perform_redir(&redir)
            .now_or_never()
            .unwrap()
            .unwrap_err();

        assert_eq!(
            e.cause,
            ErrorCause::FdNotOverwritten(Fd(999999999), Errno::EBADF)
        );
        assert_eq!(e.location, redir.body.operand().location);
        let e = env.system.write(Fd(3), &[0x20]).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn file_append_creates_empty_file() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);

        let redir = "3>> foo".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();
        env.system.write(Fd(3), &[42, 123, 57]).unwrap();

        let file = state.borrow().file_system.get("foo").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], [42, 123, 57]);
        });
    }

    #[test]
    fn file_append_appends_to_existing_file() {
        let file = Rc::new(RefCell::new(INode::new(*b"one\n")));
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", Rc::clone(&file)).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);

        let redir = ">> foo".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();
        env.system.write(Fd::STDOUT, "two\n".as_bytes()).unwrap();

        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(std::str::from_utf8(content), Ok("one\ntwo\n"));
        });
    }

    #[test]
    fn file_append_closes_opened_file_on_error() {
        let mut env = Env::new_virtual();
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999>>foo".parse().unwrap();
        let e = env
            .perform_redir(&redir)
            .now_or_never()
            .unwrap()
            .unwrap_err();

        assert_eq!(
            e.cause,
            ErrorCause::FdNotOverwritten(Fd(999999999), Errno::EBADF)
        );
        assert_eq!(e.location, redir.body.operand().location);
        let e = env.system.write(Fd(3), &[0x20]).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn file_in_out_creates_empty_file() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3<> foo".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();
        env.system.write(Fd(3), &[230, 175, 26]).unwrap();

        let file = state.borrow().file_system.get("foo").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], [230, 175, 26]);
        });
    }

    #[test]
    fn file_in_out_leaves_existing_file_content() {
        let system = VirtualSystem::new();
        let file = Rc::new(RefCell::new(INode::new([132, 79, 210])));
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", file).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3<> foo".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();

        let mut buffer = [0; 4];
        let read_count = env.system.read(Fd(3), &mut buffer).unwrap();
        assert_eq!(read_count, 3);
        assert_eq!(buffer, [132, 79, 210, 0]);
    }

    #[test]
    fn file_in_out_closes_opened_file_on_error() {
        let mut env = Env::new_virtual();
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999<>foo".parse().unwrap();
        let e = env
            .perform_redir(&redir)
            .now_or_never()
            .unwrap()
            .unwrap_err();

        assert_eq!(
            e.cause,
            ErrorCause::FdNotOverwritten(Fd(999999999), Errno::EBADF)
        );
        assert_eq!(e.location, redir.body.operand().location);
        let e = env.system.write(Fd(3), &[0x20]).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn fd_in_copies_fd() {
        for fd in [Fd(0), Fd(3)] {
            let system = VirtualSystem::new();
            let state = Rc::clone(&system.state);
            state
                .borrow_mut()
                .file_system
                .get("/dev/stdin")
                .unwrap()
                .borrow_mut()
                .body = FileBody::new([1, 2, 42]);
            let mut env = Env::with_system(Box::new(system));
            let mut env = RedirGuard::new(&mut env);
            let redir = "3<& 0".parse().unwrap();
            env.perform_redir(&redir).now_or_never().unwrap().unwrap();

            let mut buffer = [0; 4];
            let read_count = env.system.read(fd, &mut buffer).unwrap();
            assert_eq!(read_count, 3);
            assert_eq!(buffer, [1, 2, 42, 0]);
        }
    }

    #[test]
    fn fd_in_closes_fd() {
        let mut env = Env::new_virtual();
        let mut env = RedirGuard::new(&mut env);
        let redir = "<& -".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();

        let mut buffer = [0; 1];
        let e = env.system.read(Fd::STDIN, &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn fd_in_rejects_unreadable_fd() {
        let mut env = Env::new_virtual();
        let mut env = RedirGuard::new(&mut env);
        let redir = "3>foo".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();

        let redir = "<&3".parse().unwrap();
        let e = env
            .perform_redir(&redir)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(e.cause, ErrorCause::UnreadableFd(Fd(3)));
        assert_eq!(e.location, redir.body.operand().location);
    }

    #[test]
    fn keep_target_fd_open_on_error_in_fd_in() {
        let mut env = Env::new_virtual();
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999<&0".parse().unwrap();
        let e = env
            .perform_redir(&redir)
            .now_or_never()
            .unwrap()
            .unwrap_err();

        assert_eq!(
            e.cause,
            ErrorCause::FdNotOverwritten(Fd(999999999), Errno::EBADF)
        );
        assert_eq!(e.location, redir.body.operand().location);
        let mut buffer = [0; 1];
        let read_count = env.system.read(Fd(0), &mut buffer).unwrap();
        assert_eq!(read_count, 0);
    }

    #[test]
    fn fd_out_copies_fd() {
        for fd in [Fd(1), Fd(4)] {
            let system = VirtualSystem::new();
            let state = Rc::clone(&system.state);
            let mut env = Env::with_system(Box::new(system));
            let mut env = RedirGuard::new(&mut env);
            let redir = "4>& 1".parse().unwrap();
            env.perform_redir(&redir).now_or_never().unwrap().unwrap();

            env.system.write(fd, &[7, 6, 91]).unwrap();
            let file = state.borrow().file_system.get("/dev/stdout").unwrap();
            let file = file.borrow();
            assert_matches!(&file.body, FileBody::Regular { content, .. } => {
                assert_eq!(content[..], [7, 6, 91]);
            });
        }
    }

    #[test]
    fn fd_out_closes_fd() {
        let mut env = Env::new_virtual();
        let mut env = RedirGuard::new(&mut env);
        let redir = ">& -".parse().unwrap();
        env.perform_redir(&redir).now_or_never().unwrap().unwrap();

        let mut buffer = [0; 1];
        let e = env.system.read(Fd::STDOUT, &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn keep_target_fd_open_on_error_in_fd_out() {
        let mut env = Env::new_virtual();
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999>&1".parse().unwrap();
        let e = env
            .perform_redir(&redir)
            .now_or_never()
            .unwrap()
            .unwrap_err();

        assert_eq!(
            e.cause,
            ErrorCause::FdNotOverwritten(Fd(999999999), Errno::EBADF)
        );
        assert_eq!(e.location, redir.body.operand().location);
        let write_count = env.system.write(Fd(1), &[0x20]).unwrap();
        assert_eq!(write_count, 1);
    }
}
