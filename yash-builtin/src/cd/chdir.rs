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

use crate::common::print_failure_message;
use crate::common::AsStderr;
use crate::common::BuiltinEnv;
use std::ffi::CString;
use std::ffi::NulError;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use thiserror::Error;
use yash_env::semantics::Field;
use yash_env::system::Errno;
use yash_env::Env;
use yash_env::System;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;

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
    let c_path = CString::new(path.to_owned().into_os_string().into_vec())?;
    Ok(env.system.chdir(&c_path)?)
}

/// Prints an error message to the standard error.
pub async fn print_failure<E>(
    env: &mut E,
    operand: Option<&Field>,
    path: &Path,
    error: &Error,
) -> crate::Result
where
    E: BuiltinEnv + AsStderr,
{
    let field = operand.unwrap_or_else(|| env.builtin_name());
    let location = field.origin.clone();

    let error = Annotation::new(AnnotationType::Error, error.to_string().into(), &location);

    let info_label = format!("target directory is {path:?}").into();
    let info = Annotation::new(AnnotationType::Info, info_label, &location);

    let message = Message {
        r#type: AnnotationType::Error,
        title: "cannot change the working directory".into(),
        annotations: vec![error, info],
    };
    print_failure_message(env, message).await
}
