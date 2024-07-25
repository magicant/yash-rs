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
//! If the `Clobber` [shell option](yash_env::option::Option) is off and a
//! regular file exists at the target pathname, then `FileOut` will fail.
//!
//! If the body is `HereDoc`, the redirection opens a readable file descriptor
//! that yields [expansion](crate::expansion) of the content. The current
//! implementation uses an unnamed temporary file for the file descriptor, but
//! we may change the behavior in the future.
//!
//! # Performing redirections
//!
//! To perform redirections, you need to wrap an [`Env`] in a [`RedirGuard`]
//! first. Then, you call [`RedirGuard::perform_redir`] to affect the target
//! file descriptor. When you drop the `RedirGuard`, it undoes the effect to the
//! file descriptor. See the documentation for [`RedirGuard`] for details.
//!
//! # The CLOEXEC flag
//!
//! The shell may open file descriptors to accomplish its tasks. For example,
//! the dot built-in opens an FD to read a script file. Such FDs should be
//! invisible to the user, so the shell should set the CLOEXEC flag on the FDs.
//!
//! When the user tries to redirect an FD with the CLOEXEC flag, it fails with a
//! [`ReservedFd`](ErrorCause::ReservedFd) error to protect the FD from being
//! overwritten.
//!
//! Note that POSIX requires FDs between 0 and 9 (inclusive) to be available for
//! the user. The shell should move an FD to 10 or above before setting its
//! CLOEXEC flag. Also note that the above described behavior about the CLOEXEC
//! flag is specific to this implementation.

use crate::expansion::expand_text;
use crate::expansion::expand_word;
use crate::xtrace::XTrace;
use enumset::enum_set;
use enumset::EnumSet;
use std::borrow::Cow;
use std::ffi::CString;
use std::ffi::NulError;
use std::fmt::Write;
use std::num::ParseIntError;
use std::ops::Deref;
use std::ops::DerefMut;
use thiserror::Error;
use yash_env::io::Fd;
use yash_env::io::MIN_INTERNAL_FD;
use yash_env::option::Option::Clobber;
use yash_env::option::State::Off;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::system::Errno;
use yash_env::system::FdFlag;
use yash_env::system::Mode2;
use yash_env::system::OFlag;
use yash_env::system::OfdAccess;
use yash_env::system::OpenFlag;
use yash_env::system::SFlag;
use yash_env::Env;
use yash_env::System;
use yash_quote::quoted;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;
use yash_syntax::source::Location;
use yash_syntax::syntax::HereDoc;
use yash_syntax::syntax::Redir;
use yash_syntax::syntax::RedirBody;
use yash_syntax::syntax::RedirOp;
use yash_syntax::syntax::Unquote;

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
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ErrorCause {
    /// Expansion error.
    #[error(transparent)]
    Expansion(#[from] crate::expansion::ErrorCause),

    /// Pathname containing a nul byte.
    #[error(transparent)]
    NulByte(#[from] NulError),

    /// The target file descriptor could not be modified for the redirection.
    #[error("{1}")]
    FdNotOverwritten(Fd, Errno),

    /// Use of an FD reserved by the shell
    ///
    /// This error occurs when a redirection tries to modify an existing FD with
    /// the CLOEXEC flag set. See the [module documentation](self) for details.
    #[error("file descriptor {0} is reserved by the shell")]
    ReservedFd(Fd),

    /// Error while opening a file.
    ///
    /// The `CString` is the pathname of the file that could not be opened.
    #[error("cannot open file '{}': {}", .0.to_string_lossy(), .1)]
    OpenFile(CString, Errno),

    /// Operand of `<&` or `>&` that cannot be parsed as an integer.
    #[error("{0} is not a valid file descriptor: {1}")]
    MalformedFd(String, ParseIntError),

    /// `<&` applied to an unreadable file descriptor
    #[error("{0} is not a readable file descriptor")]
    UnreadableFd(Fd),

    /// `>&` applied to an unwritable file descriptor
    #[error("{0} is not a writable file descriptor")]
    UnwritableFd(Fd),

    /// Error preparing a temporary file to save here-document content
    #[error("cannot prepare temporary file for here-document: {0}")]
    TemporaryFileUnavailable(Errno),
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
            FdNotOverwritten(_, _) | ReservedFd(_) => "cannot redirect the file descriptor",
            OpenFile(_, _) => "cannot open the file",
            MalformedFd(_, _) => "not a valid file descriptor",
            UnreadableFd(_) | UnwritableFd(_) => "cannot copy file descriptor",
            TemporaryFileUnavailable(_) => "cannot prepare here-document",
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
            FdNotOverwritten(_, errno) => errno.to_string().into(),
            ReservedFd(fd) => format!("file descriptor {fd} reserved by shell").into(),
            OpenFile(path, errno) => format!("{}: {}", path.to_string_lossy(), errno).into(),
            MalformedFd(value, error) => format!("{value}: {error}").into(),
            UnreadableFd(fd) => format!("{fd}: not a readable file descriptor").into(),
            UnwritableFd(fd) => format!("{fd}: not a writable file descriptor").into(),
            TemporaryFileUnavailable(errno) => errno.to_string().into(),
        }
    }
}

/// Explanation of a redirection error.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{cause}")]
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

const MODE: Mode2 = Mode2(0o666);

fn is_cloexec(env: &Env, fd: Fd) -> bool {
    matches!(env.system.fcntl_getfd(fd), Ok(flags) if flags.contains(FdFlag::FD_CLOEXEC))
}

fn into_c_string_value_and_origin(field: Field) -> Result<(CString, Location), Error> {
    match CString::new(field.value) {
        Ok(value) => Ok((value, field.origin)),
        Err(e) => Err(Error {
            cause: ErrorCause::NulByte(e),
            location: field.origin,
        }),
    }
}

/// Opens a file for redirection.
fn open_file(
    env: &mut Env,
    access: OfdAccess,
    flags: EnumSet<OpenFlag>,
    path: Field,
) -> Result<(FdSpec, Location), Error> {
    let system = &mut env.system;
    let (path, origin) = into_c_string_value_and_origin(path)?;
    match system.open2(&path, access, flags, MODE) {
        Ok(fd) => Ok((FdSpec::Owned(fd), origin)),
        Err(errno) => Err(Error {
            cause: ErrorCause::OpenFile(path, errno),
            location: origin,
        }),
    }
}

/// Opens a file for writing with the `noclobber` option.
fn open_file_noclobber(env: &mut Env, path: Field) -> Result<(FdSpec, Location), Error> {
    let system = &mut env.system;
    let (path, origin) = into_c_string_value_and_origin(path)?;

    const FLAGS_EXCL: EnumSet<OpenFlag> = enum_set!(OpenFlag::Create | OpenFlag::Exclusive);
    match system.open2(&path, OfdAccess::WriteOnly, FLAGS_EXCL, MODE) {
        Ok(fd) => return Ok((FdSpec::Owned(fd), origin)),
        Err(Errno::EEXIST) => (),
        Err(errno) => {
            return Err(Error {
                cause: ErrorCause::OpenFile(path, errno),
                location: origin,
            })
        }
    }

    // Okay, it seems there is an existing file. Try opening it.
    match system.open2(&path, OfdAccess::WriteOnly, EnumSet::empty(), MODE) {
        Ok(fd) => {
            let is_regular = matches!(system.fstat(fd), Ok(stat)
                    if SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT == SFlag::S_IFREG);
            if is_regular {
                // We opened the FD without the O_CREAT flag, so somebody else
                // must have created this file. Failure.
                let _: Result<_, _> = system.close(fd);
                Err(Error {
                    cause: ErrorCause::OpenFile(path, Errno::EEXIST),
                    location: origin,
                })
            } else {
                Ok((FdSpec::Owned(fd), origin))
            }
        }
        Err(Errno::ENOENT) => {
            // A file existed on the first open but not on the second. There are
            // two possibilities: One is that a file existed on the first open
            // call and had been removed before the second. In this case, we
            // might be able to create another if we start over. The other is
            // that there is a symbolic link pointing to nothing, in which case
            // retrying would only lead to the same result. Since there is no
            // reliable way to tell the situations apart atomically, we give up
            // and return the initial error.
            Err(Error {
                cause: ErrorCause::OpenFile(path, Errno::EEXIST),
                location: origin,
            })
        }
        Err(errno) => Err(Error {
            cause: ErrorCause::OpenFile(path, errno),
            location: origin,
        }),
    }
}

/// Parses the target of `<&` and `>&`.
fn copy_fd(
    env: &mut Env,
    target: Field,
    expected_mode: OFlag,
) -> Result<(FdSpec, Location), Error> {
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
    fn is_fd_valid(env: &mut Env, fd: Fd, expected_mode: OFlag) -> bool {
        matches!(env.system.fcntl_getfl(fd), Ok(flags) if {
            let mode = flags & OFlag::O_ACCMODE;
            mode == expected_mode || mode == OFlag::O_RDWR
        })
    }
    fn fd_mode_error(
        fd: Fd,
        expected_mode: OFlag,
        target: Field,
    ) -> Result<(FdSpec, Location), Error> {
        let cause = match expected_mode {
            OFlag::O_RDONLY => ErrorCause::UnreadableFd(fd),
            OFlag::O_WRONLY => ErrorCause::UnwritableFd(fd),
            _ => unreachable!("unexpected mode {:?}", expected_mode),
        };
        let location = target.origin;
        Err(Error { cause, location })
    }
    if !is_fd_valid(env, fd, expected_mode) {
        return fd_mode_error(fd, expected_mode, target);
    }

    // Ensure the FD has no CLOEXEC flag
    if is_cloexec(env, fd) {
        return Err(Error {
            cause: ErrorCause::ReservedFd(fd),
            location: target.origin,
        });
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
        FileIn => open_file(env, OfdAccess::ReadOnly, EnumSet::empty(), operand),
        FileOut if env.options.get(Clobber) == Off => open_file_noclobber(env, operand),
        FileOut | FileClobber => open_file(
            env,
            OfdAccess::WriteOnly,
            OpenFlag::Create | OpenFlag::Truncate,
            operand,
        ),
        FileAppend => open_file(
            env,
            OfdAccess::WriteOnly,
            OpenFlag::Create | OpenFlag::Append,
            operand,
        ),
        FileInOut => open_file(env, OfdAccess::ReadWrite, OpenFlag::Create.into(), operand),
        FdIn => copy_fd(env, operand, OFlag::O_RDONLY),
        FdOut => copy_fd(env, operand, OFlag::O_WRONLY),
        Pipe => todo!("pipe redirection: {:?}", operand.value),
        String => todo!("here-string: {:?}", operand.value),
    }
}

/// Prepares xtrace for a normal redirection.
fn trace_normal(xtrace: Option<&mut XTrace>, target_fd: Fd, operator: RedirOp, operand: &Field) {
    if let Some(xtrace) = xtrace {
        write!(
            xtrace.redirs(),
            "{}{}{} ",
            target_fd,
            operator,
            quoted(&operand.value)
        )
        .unwrap();
    }
}

/// Prepares xtrace for a here-document.
fn trace_here_doc(xtrace: Option<&mut XTrace>, target_fd: Fd, here_doc: &HereDoc, content: &str) {
    if let Some(xtrace) = xtrace {
        write!(xtrace.redirs(), "{target_fd}{here_doc} ").unwrap();
        let (delimiter, _is_quoted) = here_doc.delimiter.unquote();
        writeln!(xtrace.here_doc_contents(), "{content}{delimiter}").unwrap();
    }
}

mod here_doc;

/// Performs a redirection.
#[allow(clippy::await_holding_refcell_ref)]
async fn perform(
    env: &mut Env,
    redir: &Redir,
    xtrace: Option<&mut XTrace>,
) -> Result<(SavedFd, Option<ExitStatus>), Error> {
    let target_fd = redir.fd_or_default();

    // Make sure target_fd doesn't have the CLOEXEC flag
    if is_cloexec(env, target_fd) {
        return Err(Error {
            cause: ErrorCause::ReservedFd(target_fd),
            location: redir.body.operand().location.clone(),
        });
    }

    // Save the current open file description at target_fd to a new FD
    let save = match env
        .system
        .dup(target_fd, MIN_INTERNAL_FD, FdFlag::FD_CLOEXEC)
    {
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
            trace_normal(xtrace, target_fd, *operator, &expansion);
            let (fd, location) = open_normal(env, *operator, expansion).await?;
            (fd, location, exit_status)
        }
        RedirBody::HereDoc(here_doc) => {
            let content_ref = here_doc.content.get();
            let content = content_ref.map(Cow::Borrowed).unwrap_or_default();
            let (content, exit_status) = expand_text(env, &content).await?;
            trace_here_doc(xtrace, target_fd, here_doc, &content);
            let location = here_doc.delimiter.location.clone();
            match here_doc::open_fd(env, content).await {
                Ok(fd) => (FdSpec::Owned(fd), location, exit_status),
                Err(cause) => return Err(Error { cause, location }),
            }
        }
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
    ///
    /// If `xtrace` is `Some` instance of `XTrace`, the redirection operators
    /// and the expanded operands are written to it.
    pub async fn perform_redir(
        &mut self,
        redir: &Redir,
        xtrace: Option<&mut XTrace>,
    ) -> Result<Option<ExitStatus>, Error> {
        let (saved_fd, exit_status) = perform(self, redir, xtrace).await?;
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
    ///
    /// If `xtrace` is `Some` instance of `XTrace`, the redirection operators
    /// and the expanded operands are written to it.
    pub async fn perform_redirs<'a, I>(
        &mut self,
        redirs: I,
        mut xtrace: Option<&mut XTrace>,
    ) -> Result<Option<ExitStatus>, Error>
    where
        I: IntoIterator<Item = &'a Redir>,
    {
        let mut exit_status = None;
        for redir in redirs {
            let new_exit_status = self.perform_redir(redir, xtrace.as_deref_mut()).await?;
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
        for SavedFd { original: _, save } in self.saved_fds.drain(..) {
            if let Some(save) = save {
                let _: Result<_, _> = self.env.system.close(save);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use std::rc::Rc;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::r#virtual::INode;
    use yash_env::system::resource::LimitPair;
    use yash_env::system::resource::Resource;
    use yash_env::Env;
    use yash_env::VirtualSystem;
    use yash_env_test_helper::in_virtual_system;
    use yash_syntax::syntax::Text;

    /// Returns a virtual system with a file descriptor limit.
    fn system_with_nofile_limit() -> VirtualSystem {
        let mut system = VirtualSystem::new();
        system
            .setrlimit(
                Resource::NOFILE,
                LimitPair {
                    soft: 1024,
                    hard: 1024,
                },
            )
            .unwrap();
        system
    }

    #[test]
    fn basic_file_in_redirection() {
        let system = system_with_nofile_limit();
        let file = Rc::new(RefCell::new(INode::new([42, 123, 254])));
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", file).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3< foo".parse().unwrap();
        let result = env
            .perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(result, None);

        let mut buffer = [0; 4];
        let read_count = env.system.read(Fd(3), &mut buffer).unwrap();
        assert_eq!(read_count, 3);
        assert_eq!(buffer, [42, 123, 254, 0]);
    }

    #[test]
    fn moving_fd() {
        let system = system_with_nofile_limit();
        let file = Rc::new(RefCell::new(INode::new([42, 123, 254])));
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", file).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "< foo".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut buffer = [0; 4];
        let read_count = env.system.read(Fd::STDIN, &mut buffer).unwrap();
        assert_eq!(read_count, 3);
        assert_eq!(buffer, [42, 123, 254, 0]);

        let e = env.system.read(Fd(3), &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn saving_and_undoing_fd() {
        let system = system_with_nofile_limit();
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
            .perform_redir(&redir, None)
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
    fn preserving_fd() {
        let system = system_with_nofile_limit();
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
            .perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();
        redir_env.preserve_redirs();
        drop(redir_env);

        let mut buffer = [0; 2];
        let read_count = env.system.read(Fd::STDIN, &mut buffer).unwrap();
        assert_eq!(read_count, 0);
        let e = env.system.read(MIN_INTERNAL_FD, &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn undoing_without_initial_fd() {
        let system = system_with_nofile_limit();
        let mut state = system.state.borrow_mut();
        state.file_system.save("input", Rc::default()).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut redir_env = RedirGuard::new(&mut env);
        let redir = "4< input".parse().unwrap();
        redir_env
            .perform_redir(&redir, None)
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
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = "< no_such_file".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::OpenFile(c"no_such_file".to_owned(), Errno::ENOENT)
        );
        assert_eq!(e.location, redir.body.operand().location);
    }

    #[test]
    fn multiple_redirections() {
        let system = system_with_nofile_limit();
        let mut state = system.state.borrow_mut();
        let file = Rc::new(RefCell::new(INode::new([100])));
        state.file_system.save("foo", file).unwrap();
        let file = Rc::new(RefCell::new(INode::new([200])));
        state.file_system.save("bar", file).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        env.perform_redir(&"< foo".parse().unwrap(), None)
            .now_or_never()
            .unwrap()
            .unwrap();
        env.perform_redir(&"3< bar".parse().unwrap(), None)
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
        let system = system_with_nofile_limit();
        let mut state = system.state.borrow_mut();
        let file = Rc::new(RefCell::new(INode::new([100])));
        state.file_system.save("foo", file).unwrap();
        let file = Rc::new(RefCell::new(INode::new([200])));
        state.file_system.save("bar", file).unwrap();
        drop(state);

        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        env.perform_redir(&"< foo".parse().unwrap(), None)
            .now_or_never()
            .unwrap()
            .unwrap();
        env.perform_redir(&"< bar".parse().unwrap(), None)
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut buffer = [0; 1];
        let read_count = env.system.read(Fd::STDIN, &mut buffer).unwrap();
        assert_eq!(read_count, 1);
        assert_eq!(buffer, [200]);
    }

    #[test]
    fn target_with_cloexec() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let fd = env
            .system
            .open2(
                c"foo",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode2(0o777),
            )
            .unwrap();
        env.system.fcntl_setfd(fd, FdFlag::FD_CLOEXEC).unwrap();

        let mut env = RedirGuard::new(&mut env);
        let redir = format!("{fd}> bar").parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(e.cause, ErrorCause::ReservedFd(fd));
        assert_eq!(e.location, redir.body.operand().location);
    }

    #[test]
    fn exit_status_of_command_substitution_in_normal() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.builtins.insert("return", return_builtin());
            let mut env = RedirGuard::new(&mut env);
            let redir = "3> $(echo foo; return -n 79)".parse().unwrap();
            let result = env.perform_redir(&redir, None).await.unwrap();
            assert_eq!(result, Some(ExitStatus(79)));
            let file = state.borrow().file_system.get("foo");
            assert!(file.is_ok(), "{file:?}");
        })
    }

    #[test]
    fn exit_status_of_command_substitution_in_here_doc() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.builtins.insert("return", return_builtin());
            let mut env = RedirGuard::new(&mut env);
            let redir = Redir {
                fd: Some(Fd(4)),
                body: RedirBody::HereDoc(Rc::new(HereDoc {
                    delimiter: "-END".parse().unwrap(),
                    remove_tabs: false,
                    content: "$(echo foo)$(echo bar; return -n 42)\n"
                        .parse::<Text>()
                        .unwrap()
                        .into(),
                })),
            };
            let result = env.perform_redir(&redir, None).await.unwrap();
            assert_eq!(result, Some(ExitStatus(42)));

            let mut buffer = [0; 10];
            let count = env.system.read(Fd(4), &mut buffer).unwrap();
            assert_eq!(count, 7);
            assert_eq!(&buffer[..7], b"foobar\n");
        })
    }

    #[test]
    fn xtrace_normal() {
        let mut xtrace = XTrace::new();
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        env.perform_redir(&"> foo${unset-&}".parse().unwrap(), Some(&mut xtrace))
            .now_or_never()
            .unwrap()
            .unwrap();
        env.perform_redir(&"3>> bar".parse().unwrap(), Some(&mut xtrace))
            .now_or_never()
            .unwrap()
            .unwrap();
        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "1>'foo&' 3>>bar\n");
    }

    #[test]
    fn xtrace_here_doc() {
        let mut xtrace = XTrace::new();
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);

        let redir = Redir {
            fd: Some(Fd(4)),
            body: RedirBody::HereDoc(Rc::new(HereDoc {
                delimiter: r"-\END".parse().unwrap(),
                remove_tabs: false,
                content: "foo\n".parse::<Text>().unwrap().into(),
            })),
        };
        env.perform_redir(&redir, Some(&mut xtrace))
            .now_or_never()
            .unwrap()
            .unwrap();

        let redir = Redir {
            fd: Some(Fd(5)),
            body: RedirBody::HereDoc(Rc::new(HereDoc {
                delimiter: r"EOF".parse().unwrap(),
                remove_tabs: false,
                content: "bar${unset-}\n".parse::<Text>().unwrap().into(),
            })),
        };
        env.perform_redir(&redir, Some(&mut xtrace))
            .now_or_never()
            .unwrap()
            .unwrap();

        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "4<< -\\END 5<<EOF\nfoo\n-END\nbar\nEOF\n");
    }

    #[test]
    fn file_in_closes_opened_file_on_error() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999</dev/stdin".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
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
        let system = system_with_nofile_limit();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3> foo".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();
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
        let system = system_with_nofile_limit();
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", Rc::clone(&file)).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);

        let redir = "3> foo".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();

        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], []);
        });
    }

    #[test]
    fn file_out_noclobber_with_regular_file() {
        let file = Rc::new(RefCell::new(INode::new([42, 123, 254])));
        let system = system_with_nofile_limit();
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", Rc::clone(&file)).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        env.options.set(Clobber, Off);
        let mut env = RedirGuard::new(&mut env);

        let redir = "3> foo".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap_err();

        assert_eq!(
            e.cause,
            ErrorCause::OpenFile(c"foo".to_owned(), Errno::EEXIST)
        );
        assert_eq!(e.location, redir.body.operand().location);
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], [42, 123, 254]);
        });
    }

    #[test]
    fn file_out_noclobber_with_non_regular_file() {
        let inode = INode {
            body: FileBody::Fifo {
                content: Default::default(),
                readers: 1,
                writers: 0,
            },
            permissions: Default::default(),
        };
        let file = Rc::new(RefCell::new(inode));
        let system = system_with_nofile_limit();
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", Rc::clone(&file)).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        env.options.set(Clobber, Off);
        let mut env = RedirGuard::new(&mut env);

        let redir = "3> foo".parse().unwrap();
        let result = env.perform_redir(&redir, None).now_or_never().unwrap();
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn file_out_closes_opened_file_on_error() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999>foo".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
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
        let system = system_with_nofile_limit();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);

        let redir = "3>| foo".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();
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
        let system = system_with_nofile_limit();
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", Rc::clone(&file)).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);

        let redir = "3>| foo".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();

        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], []);
        });
    }

    // TODO file_clobber_with_noclobber_fails_with_existing_file

    #[test]
    fn file_clobber_closes_opened_file_on_error() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999>|foo".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
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
        let system = system_with_nofile_limit();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);

        let redir = "3>> foo".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();
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
        let system = system_with_nofile_limit();
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", Rc::clone(&file)).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);

        let redir = ">> foo".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();
        env.system.write(Fd::STDOUT, "two\n".as_bytes()).unwrap();

        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(std::str::from_utf8(content), Ok("one\ntwo\n"));
        });
    }

    #[test]
    fn file_append_closes_opened_file_on_error() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999>>foo".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
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
        let system = system_with_nofile_limit();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3<> foo".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();
        env.system.write(Fd(3), &[230, 175, 26]).unwrap();

        let file = state.borrow().file_system.get("foo").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], [230, 175, 26]);
        });
    }

    #[test]
    fn file_in_out_leaves_existing_file_content() {
        let system = system_with_nofile_limit();
        let file = Rc::new(RefCell::new(INode::new([132, 79, 210])));
        let mut state = system.state.borrow_mut();
        state.file_system.save("foo", file).unwrap();
        drop(state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3<> foo".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut buffer = [0; 4];
        let read_count = env.system.read(Fd(3), &mut buffer).unwrap();
        assert_eq!(read_count, 3);
        assert_eq!(buffer, [132, 79, 210, 0]);
    }

    #[test]
    fn file_in_out_closes_opened_file_on_error() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999<>foo".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
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
            let system = system_with_nofile_limit();
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
            env.perform_redir(&redir, None)
                .now_or_never()
                .unwrap()
                .unwrap();

            let mut buffer = [0; 4];
            let read_count = env.system.read(fd, &mut buffer).unwrap();
            assert_eq!(read_count, 3);
            assert_eq!(buffer, [1, 2, 42, 0]);
        }
    }

    #[test]
    fn fd_in_closes_fd() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = "<& -".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut buffer = [0; 1];
        let e = env.system.read(Fd::STDIN, &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn fd_in_rejects_unreadable_fd() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3>foo".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();

        let redir = "<&3".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(e.cause, ErrorCause::UnreadableFd(Fd(3)));
        assert_eq!(e.location, redir.body.operand().location);
    }

    #[test]
    fn fd_in_rejects_unopened_fd() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);

        let redir = "3<&3".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(e.cause, ErrorCause::UnreadableFd(Fd(3)));
        assert_eq!(e.location, redir.body.operand().location);
    }

    #[test]
    fn fd_in_rejects_fd_with_cloexec() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        env.system.fcntl_setfd(Fd(0), FdFlag::FD_CLOEXEC).unwrap();

        let mut env = RedirGuard::new(&mut env);
        let redir = "3<& 0".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(e.cause, ErrorCause::ReservedFd(Fd(0)));
        assert_eq!(e.location, redir.body.operand().location);
    }

    #[test]
    fn keep_target_fd_open_on_error_in_fd_in() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999<&0".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
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
            let system = system_with_nofile_limit();
            let state = Rc::clone(&system.state);
            let mut env = Env::with_system(Box::new(system));
            let mut env = RedirGuard::new(&mut env);
            let redir = "4>& 1".parse().unwrap();
            env.perform_redir(&redir, None)
                .now_or_never()
                .unwrap()
                .unwrap();

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
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = ">& -".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut buffer = [0; 1];
        let e = env.system.read(Fd::STDOUT, &mut buffer).unwrap_err();
        assert_eq!(e, Errno::EBADF);
    }

    #[test]
    fn fd_out_rejects_unwritable_fd() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = "3</dev/stdin".parse().unwrap();
        env.perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap();

        let redir = ">&3".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(e.cause, ErrorCause::UnwritableFd(Fd(3)));
        assert_eq!(e.location, redir.body.operand().location);
    }

    #[test]
    fn fd_out_rejects_unopened_fd() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);

        let redir = "3>&3".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(e.cause, ErrorCause::UnwritableFd(Fd(3)));
        assert_eq!(e.location, redir.body.operand().location);
    }

    #[test]
    fn fd_out_rejects_fd_with_cloexec() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        env.system.fcntl_setfd(Fd(1), FdFlag::FD_CLOEXEC).unwrap();

        let mut env = RedirGuard::new(&mut env);
        let redir = "4>& 1".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(e.cause, ErrorCause::ReservedFd(Fd(1)));
        assert_eq!(e.location, redir.body.operand().location);
    }

    #[test]
    fn keep_target_fd_open_on_error_in_fd_out() {
        let mut env = Env::with_system(Box::new(system_with_nofile_limit()));
        let mut env = RedirGuard::new(&mut env);
        let redir = "999999999>&1".parse().unwrap();
        let e = env
            .perform_redir(&redir, None)
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
