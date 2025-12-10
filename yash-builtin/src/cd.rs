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
//! This module implements the [`cd` built-in], which changes the working directory.
//!
//! [`cd` built-in]: https://magicant.github.io/yash-rs/builtins/cd.html

use crate::Result;
use crate::common::report::report;
use yash_env::Env;
use yash_env::path::Path;
use yash_env::path::PathBuf;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::source::pretty::{Footnote, FootnoteType, Report, ReportType};
use yash_env::system::Errno;
use yash_env::system::System;
use yash_env::variable::PWD;

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

fn get_pwd<S>(env: &Env<S>) -> String {
    env.variables.get_scalar(PWD).unwrap_or_default().to_owned()
}

/// Reports that the new `$PWD` value cannot be determined, and returns the
/// corresponding exit status.
async fn report_pwd_error<S: System>(env: &mut Env<S>, errno: Errno, ensure_pwd: bool) -> Result {
    let (r#type, exit_status) = if ensure_pwd {
        (ReportType::Error, EXIT_STATUS_STALE_PWD)
    } else {
        (ReportType::Warning, EXIT_STATUS_SUCCESS)
    };

    let mut report = Report::new();
    report.r#type = r#type;
    report.title = "cannot compute new $PWD".into();
    report.footnotes.push(Footnote {
        r#type: FootnoteType::Note,
        label: format!("error from underlying system call: {errno}").into(),
    });
    self::report(env, report, exit_status).await
}

/// Entry point for executing the `cd` built-in
///
/// This function uses functions in the submodules to execute the built-in.
pub async fn main<S: System>(env: &mut Env<S>, args: Vec<Field>) -> Result {
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
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
