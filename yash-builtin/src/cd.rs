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

//! Cd built-in
//!
//! The **`cd`** built-in changes the working directory.
//!
//! # Synopsis
//!
//! ```sh
//! cd [-L|-P [-e]] [directory]
//! ```
//!
//! # Description
//!
//! The built-in changes the working directory to the specified directory. The
//! new working directory is determined from the option and operand as follows:
//!
//! 1. If the operand is omitted, the value of `$HOME` is used for the operand.
//!    If the operand is a single hyphen (`-`), the value of `$OLDPWD` is used
//!    for the operand. If the variable is not set or empty, it is an error.
//!    Otherwise, the operand is used as is.
//! 2. If the operand does not start with a slash (`/`) and the first pathname
//!    component in the operand is neither dot (`.`) nor dot-dot (`..`), the
//!    built-in searches the directories specified by the `$CDPATH` variable for
//!    a first directory that contains the operand as a subdirectory.
//!    If such a directory is found, the operand is replaced with the path to
//!    the subdirectory, that is, the concatenation of the directory contained
//!    in `$CDPATH` and the previous operand.
//!    If no such directory is found, the operand is used as is.
//!    (See below for security implications of `$CDPATH`.)
//! 3. If the `-L` option is effective, the operand is canonicalized as follows:
//!     1. If the operand does not start with a slash (`/`), the value of `$PWD`
//!        is prepended to the operand.
//!     2. The dot (`.`) components in the operand are removed.
//!     3. The dot-dot (`..`) components in the operand are removed along with
//!        the preceding component. However, if such a preceding component
//!        refers to a non-existent directory, it is an error.
//!     4. Redundant slashes in the operand are removed.
//!
//! The working directory is changed to the operand after the above processing.
//! If the change is successful, the value of `$PWD` is updated to the new
//! working directory:
//!
//! - If the `-L` option is effective, the final operand value becomes the new
//!   value of `$PWD`.
//! - If the `-P` option is effective, the new `$PWD` value is recomputed in the
//!   same way as `pwd -P` does, so it does not include symbolic links.
//!
//! The previous `$PWD` value is assigned to `$OLDPWD`.
//!
//! If the new working directory is taken from `$CDPATH` or the operand is a
//! single hyphen (`-`), the built-in prints the new value of `$PWD` followed by
//! a newline to the standard output. (TODO: This printing can be enforced or
//! suppressed with the **`--print`** option.)
//!
//! # Options
//!
//! With the **`-L`** (**`--logical`**) option, the operand is resolved
//! logically, that is, the canonicalization is performed as above. With the
//! **`-P`** (**`--physical`**) option, the operand is resolved physically; the
//! canonicalization is skipped.
//! These two options are mutually exclusive. The last specified one applies if
//! given both. The default is `-L`.
//!
//! When the `-P` option is effective, the built-in may fail to determine the
//! new working directory pathname to assign to `$PWD`. By default, the exit
//! status does not indicate the failure. If the **`-e`** (**`--ensure-pwd`**)
//! option is given together with the `-P` option, the built-in returns exit
//! status 1 in this case.
//!
//! TODO: The **`--default-directory=directory`** option is not implemented.
//!
//! TODO: The **`--print={always,auto,never}`** option is not implemented.
//!
//! # Operands
//!
//! The built-in takes a single operand that specifies the directory to change
//! to. If omitted, the value of `$HOME` is used. If the operand is a single
//! hyphen (`-`), the value of `$OLDPWD` is used.
//!
//! # Errors
//!
//! This built-in fails if the working directory cannot be changed, for example,
//! in the following cases:
//!
//! - The operand does not resolve to an existing accessible directory.
//! - The operand is omitted and `$HOME` is not set or empty.
//! - The operand is a single hyphen (`-`) and `$OLDPWD` is not set or empty.
//! - The resolved pathname of the new working directory is too long.
//!
//! It is also an error if a given operand is an empty string.
//!
//! If the `-P` option is effective, the built-in may fail to determine the
//! new working directory pathname to assign to `$PWD`, for example, in the
//! following cases:
//!
//! - The new pathname is too long.
//! - Some ancestor directories of the new working directory are not accessible.
//! - The new working directory does not belong to the filesystem tree.
//!
//! In these cases, the working directory remains changed, the `$PWD` variable
//! is left empty, and the exit status depends on the `-e` option.
//!
//! The built-in may also fail if `$PWD` or `$OLDPWD` is read-only. In this case,
//! the working directory remains changed, but the variable is not updated.
//!
//! If the new working directory name cannot be printed to the standard output,
//! the built-in prints a warning message to the standard error, but this does
//! not affect the working directory change or the exit status.
//!
//! # Exit Status
//!
//! - If the working directory is changed successfully, the exit status is zero,
//!   except in the following cases where the exit status is one:
//!   - The `-P` and `-e` options are effective and the new working directory
//!     pathname cannot be determined.
//!   - The `$PWD` or `$OLDPWD` variable is read-only.
//! - If the working directory cannot be changed because of an error in the
//!   underlying `chdir` system call, the exit status is two.
//! - If the `-L` option is effective and canonicalization fails because of a
//!   `..` component referring to a non-existent directory, the exit status is
//!   three.
//! - If the operand cannot be processed because of an unset or empty `$HOME` or
//!   `$OLDPWD`, the exit status is four.
//! - If the command arguments are invalid, the exit status is five.
//!
//! # Security considerations
//!
//! Although `$CDPATH` can be helpful if used correctly, it can catch unwary
//! users off guard, leading to unintended changes in the behavior of shell
//! scripts. If a shell script is executed with the `$CDPATH` environment
//! variable set to a directory crafted by an attacker, the script may change
//! the working directory to an unexpected one. To ensure that the cd built-in
//! behaves as intended, shell script writers should unset the variable at the
//! beginning of the script. Users can configure `$CDPATH` in their shell
//! sessions, but should avoid exporting the variable to the environment.
//!
//! # Portability
//!
//! The `-L`, `-P`, and `-e` options are defined in POSIX. The other options are
//! non-standard.
//!
//! The shell sets `$PWD` on the startup and modifies it in the cd built-in.
//! If `$PWD` is modified or unset otherwise, the behavior of the cd and
//! [pwd](crate::pwd) built-ins is unspecified.
//!
//! The error handling behavior and the exit status do not agree between
//! existing implementations when the built-in fails because of a write error or
//! a read-only variable error.
//!
//! Other implementations may return different non-zero exit statuses in cases
//! where this implementation would return exit statuses between 2 and 4.
//!
//! POSIX requires the shell to convert the pathname passed to the underlying
//! `chdir` system call to a shorter relative pathname when the `-L` option is
//! in effect. This conversion is mandatory if:
//!
//! - the original operand was not longer than PATH_MAX bytes (including the
//!   terminating nul byte),
//! - the final operand is longer than PATH_MAX bytes (including the terminating
//!   nul byte), and
//! - the final operand starts with `$PWD` and hence can be considered to be a
//!   subdirectory of the current working directory.
//!
//! POSIX does not specify whether the shell should perform the conversion if
//! the above conditions are not met. The current implementation does it if and
//! only if the final operand starts with `$PWD`.

use crate::Result;
use crate::common::report;
use yash_env::Env;
use yash_env::path::Path;
use yash_env::path::PathBuf;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::system::Errno;
use yash_env::variable::PWD;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Footer;
use yash_syntax::source::pretty::Message;

/// Exit status when the built-in succeeds
pub const EXIT_STATUS_SUCCESS: ExitStatus = ExitStatus(0);

/// Exit status when the new `$PWD` value cannot be determined
pub const EXIT_STATUS_STALE_PWD: ExitStatus = ExitStatus(1);

/// Exit status when `$PWD` or `$OLDPWD` cannot be updated
pub const EXIT_STATUS_ASSIGN_ERROR: ExitStatus = ExitStatus(1);

/// Exit status for an error in the underlying `chdir` system call
pub const EXIT_STATUS_CHDIR_ERROR: ExitStatus = ExitStatus(2);

/// Exit status for when canonicalization fails because of a `..` component
/// referring to a non-existent directory
pub const EXIT_STATUS_CANNOT_CANONICALIZE: ExitStatus = ExitStatus(3);

/// Exit status for an unset or empty `$HOME` or `$OLDPWD`
pub const EXIT_STATUS_UNSET_VARIABLE: ExitStatus = ExitStatus(4);

/// Exit status for invalid command arguments
pub const EXIT_STATUS_SYNTAX_ERROR: ExitStatus = ExitStatus(5);

/// Treatments of symbolic links in the pathname
#[derive(Debug, Clone, Copy, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Mode {
    /// Treat the pathname literally without resolving symbolic links
    #[default]
    Logical,

    /// Resolve symbolic links in the pathname
    Physical,
}

/// Parsed command line arguments
#[derive(Debug, Clone, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Command {
    /// Treatments of symbolic links in the pathname
    ///
    /// The `-L` and `-P` options are translated to this field.
    pub mode: Mode,

    /// Whether to ensure the new `$PWD` value
    ///
    /// The `-e` option is translated to this field.
    /// This option must be used together with the `-P` option.
    pub ensure_pwd: bool,

    /// The operand that specifies the directory to change to
    pub operand: Option<Field>,
}

pub mod assign;
pub mod canonicalize;
pub mod cdpath;
pub mod chdir;
pub mod print;
pub mod shorten;
pub mod syntax;
pub mod target;

fn get_pwd(env: &Env) -> String {
    env.variables.get_scalar(PWD).unwrap_or_default().to_owned()
}

/// Reports that the new `$PWD` value cannot be determined, and returns the
/// corresponding exit status.
async fn report_pwd_error(env: &mut Env, errno: Errno, ensure_pwd: bool) -> Result {
    let (r#type, exit_status) = if ensure_pwd {
        (AnnotationType::Error, EXIT_STATUS_STALE_PWD)
    } else {
        (AnnotationType::Warning, EXIT_STATUS_SUCCESS)
    };

    let message = Message {
        r#type,
        title: "cannot compute new $PWD".into(),
        annotations: vec![],
        footers: vec![Footer {
            r#type: AnnotationType::Info,
            label: format!("error from underlying system call: {errno}").into(),
        }],
    };

    report(env, message, exit_status).await
}

/// Entry point for executing the `cd` built-in
///
/// This function uses functions in the submodules to execute the built-in.
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    let command = match syntax::parse(env, args) {
        Ok(command) => command,
        Err(e) => return report(env, &e, EXIT_STATUS_SYNTAX_ERROR).await,
    };

    let pwd = get_pwd(env);

    let (path, origin) = match target::target(env, &command, &pwd) {
        Ok(target) => target,
        Err(e) => return report(env, &e, e.exit_status()).await,
    };

    let short_path = shorten::shorten(&path, Path::new(&pwd), command.mode);

    match chdir::chdir(env, short_path) {
        Ok(()) => {}
        Err(e) => return chdir::report_failure(env, command.operand.as_ref(), &path, &e).await,
    }

    let (new_pwd, result1) = match assign::new_pwd(env, command.mode, &path) {
        Ok(new_pwd) => (new_pwd, Result::from(EXIT_STATUS_SUCCESS)),
        Err(errno) => (
            PathBuf::default(),
            report_pwd_error(env, errno, command.ensure_pwd).await,
        ),
    };

    print::print_path(env, &new_pwd, &origin).await;

    let result2 = assign::set_oldpwd(env, pwd).await;
    let result3 = assign::set_pwd(env, new_pwd).await;

    result1.max(result2).max(result3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env_test_helper::assert_stderr;

    #[test]
    fn report_pwd_error_with_ensure_pwd() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);

        let result = report_pwd_error(&mut env, Errno::ENAMETOOLONG, true)
            .now_or_never()
            .unwrap();

        // Something should be printed
        assert_stderr(&state, |stderr| assert!(!stderr.is_empty()));

        assert_eq!(result, Result::from(ExitStatus(1)));
    }

    #[test]
    fn report_pwd_error_without_ensure_pwd() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);

        let result = report_pwd_error(&mut env, Errno::ENAMETOOLONG, false)
            .now_or_never()
            .unwrap();

        // Something should be printed
        assert_stderr(&state, |stderr| assert!(!stderr.is_empty()));

        assert_eq!(result, Result::from(ExitStatus(0)));
    }
}
