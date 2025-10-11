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

//! Main semantics of the pwd built-in

use super::Mode;
use thiserror::Error;
use yash_env::system::Errno;
use yash_env::{Env, System};
use yash_syntax::source::pretty::{Report, ReportType};

/// Error in running the pwd built-in
#[derive(Debug, Clone, Eq, Error, PartialEq)]
pub enum Error {
    /// Error obtaining the current working directory path
    #[error(transparent)]
    SystemError(Errno),
}

impl Error {
    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = format!("cannot determine working directory: {self}").into();
        report
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

/// Logical result of the pwd built-in
///
/// If successful, the value is the working directory path to be printed,
/// including the trailing newline. If unsuccessful, the value is an error.
pub type Result = std::result::Result<String, Error>;

/// Computes the result of the pwd built-in.
///
/// If successful, the result is the working directory path to be printed,
/// including the trailing newline.
pub fn compute(env: &Env, mode: Mode) -> Result {
    match mode {
        Mode::Logical => {
            if let Some(pwd) = env.get_pwd_if_correct() {
                return Ok(format!("{pwd}\n"));
            }
        }
        Mode::Physical => (),
    }

    let mut cwd = env
        .system
        .getcwd()
        .map_err(Error::SystemError)?
        .into_unix_string()
        .into_string()
        .map_err(|_| Error::SystemError(Errno::EILSEQ))?;
    cwd.push('\n');
    Ok(cwd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::path::PathBuf;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::r#virtual::Inode;
    use yash_env::variable::PWD;
    use yash_env::variable::Scope::Global;

    fn env_with_symlink_to_dir() -> Env {
        let mut system = Box::new(VirtualSystem::new());
        let mut state = system.state.borrow_mut();
        state
            .file_system
            .save(
                "/foo/bar/dir",
                Rc::new(RefCell::new(Inode {
                    body: FileBody::Directory {
                        files: Default::default(),
                    },
                    permissions: Default::default(),
                })),
            )
            .unwrap();
        state
            .file_system
            .save(
                "/foo/link",
                Rc::new(RefCell::new(Inode {
                    body: FileBody::Symlink {
                        target: "bar/dir".into(),
                    },
                    permissions: Default::default(),
                })),
            )
            .unwrap();
        drop(state);
        system
            .current_process_mut()
            .chdir(PathBuf::from("/foo/bar/dir"));
        Env::with_system(system)
    }

    #[test]
    fn logical_with_correct_pwd() {
        let mut env = env_with_symlink_to_dir();
        env.variables
            .get_or_new(PWD, Global)
            .assign("/foo/link", None)
            .unwrap();
        let result = compute(&env, Mode::Logical).unwrap();
        assert_eq!(result, "/foo/link\n");
    }

    #[test]
    fn logical_with_wrong_pwd() {
        let mut env = env_with_symlink_to_dir();
        env.variables
            .get_or_new(PWD, Global)
            .assign("/foo/./link", None)
            .unwrap();
        let result = compute(&env, Mode::Logical).unwrap();
        assert_eq!(result, "/foo/bar/dir\n");
    }

    #[test]
    fn physical() {
        let mut env = env_with_symlink_to_dir();
        env.variables
            .get_or_new(PWD, Global)
            .assign("/foo/link", None)
            .unwrap();
        let result = compute(&env, Mode::Physical).unwrap();
        assert_eq!(result, "/foo/bar/dir\n");
    }
}
