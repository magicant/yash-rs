// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Working directory path handling

use super::Env;
use crate::variable::ReadOnlyError;
use crate::variable::Scope::Global;
use crate::variable::Variable;
use crate::System;

/// Error in [`Env::prepare_pwd`]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparePwdError {
    /// Error assigning to the `$PWD` variable
    AssignError(ReadOnlyError),
    /// Error obtaining the current working directory path
    GetCwdError(nix::Error),
}

impl From<ReadOnlyError> for PreparePwdError {
    fn from(error: ReadOnlyError) -> Self {
        PreparePwdError::AssignError(error)
    }
}

impl From<nix::Error> for PreparePwdError {
    fn from(error: nix::Error) -> Self {
        PreparePwdError::GetCwdError(error)
    }
}

impl Env {
    /// Updates the `$PWD` variable with the current working directory.
    ///
    /// If the value of `$PWD` is a path to the current working directory and
    /// does not contain any single or double dot components, this function does
    /// not modify it. Otherwise, this function sets the value to
    /// `self.system.getcwd()`.
    pub fn prepare_pwd(&mut self) -> Result<(), PreparePwdError> {
        let dir = self
            .system
            .getcwd()?
            .into_os_string()
            .into_string()
            .map_err(|_| nix::Error::EILSEQ)?;
        self.variables
            .assign(Global, "PWD".to_string(), Variable::new(dir))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variable::Value;
    use crate::VirtualSystem;
    use std::path::PathBuf;

    #[test]
    fn prepare_pwd_no_value() {
        let mut system = Box::new(VirtualSystem::new());
        system.current_process_mut().cwd = PathBuf::from("/foo/bar/dir");
        let mut env = Env::with_system(system);
        let result = env.prepare_pwd();
        assert_eq!(result, Ok(()));

        let pwd = env.variables.get("PWD").unwrap();
        assert_eq!(pwd.value, Value::scalar("/foo/bar/dir"));
    }

    // TODO prepare_pwd_with_correct_path
    // TODO prepare_pwd_with_dot
    // TODO prepare_pwd_with_dot_dot
    // TODO prepare_pwd_with_wrong_path
}
