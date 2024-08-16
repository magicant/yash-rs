// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Part of the cd built-in that invokes the underlying system call

use crate::common::arrange_message_and_divert;
use std::borrow::Cow;
use std::ffi::CString;
use std::ffi::NulError;
use thiserror::Error;
use yash_env::path::Path;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
#[cfg(doc)]
use yash_env::stack::Stack;
use yash_env::system::Errno;
#[cfg(doc)]
use yash_env::system::SharedSystem;
use yash_env::Env;
use yash_env::System;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;

/// Error invoking the underlying system call
#[derive(Debug, Clone, Eq, Error, PartialEq)]
pub enum Error {
    /// The target path contains a nul byte.
    #[error("path contains a nul byte")]
    NulByteInPath,

    /// Error from the underlying system call
    #[error(transparent)]
    SystemError(#[from] Errno),
}

impl From<NulError> for Error {
    fn from(_: NulError) -> Self {
        Self::NulByteInPath
    }
}

pub fn chdir(env: &mut Env, path: &Path) -> Result<(), Error> {
    let c_path = CString::new(path.as_unix_str().as_bytes())?;
    Ok(env.system.chdir(&c_path)?)
}

/// Creates a message that describes the failure.
///
/// This function expects
/// [`env.stack.current_builtin()`](Stack::current_builtin) to return `Some(_)`.
/// If it returns `None`, the function will annotate the message with a dummy
/// location.
///
/// See [`arrange_message_and_divert`] for the second return value.
#[must_use]
pub fn failure_message(
    env: &Env,
    operand: Option<&Field>,
    path: &Path,
    error: &Error,
) -> (String, yash_env::semantics::Result) {
    let label = Cow::Owned(format!("{path:?}: {error}"));
    let location = operand
        .or_else(|| env.stack.current_builtin().map(|builtin| &builtin.name))
        .map(|field| Cow::Borrowed(&field.origin))
        .unwrap_or_else(|| Cow::Owned(Location::dummy("")));
    let error = Annotation::new(AnnotationType::Error, label, &location);
    let message = Message {
        r#type: AnnotationType::Error,
        title: "cannot change the working directory".into(),
        annotations: vec![error],
        footers: vec![],
    };
    arrange_message_and_divert(env, message)
}

/// Prints an error message to the standard error.
///
/// This function constructs a message with [`failure_message`] and prints it
/// with [`SharedSystem::print_error`].
pub async fn report_failure(
    env: &mut Env,
    operand: Option<&Field>,
    path: &Path,
    error: &Error,
) -> crate::Result {
    let (message, divert) = failure_message(env, operand, path, error);
    env.system.print_error(&message).await;
    crate::Result::with_exit_status_and_divert(ExitStatus::FAILURE, divert)
}
