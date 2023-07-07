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

//! Exec built-in
//!
//! The **`exec`** built-in replaces the current shell process with an external
//! utility invoked by treating the specified operands as a command. Without
//! operands, the built-in makes redirections applied to it permanent in the
//! current shell process.
//!
//! # Syntax
//!
//! ```sh
//! exec [name [arguments...]]
//! ```
//!
//! # Semantics
//!
//! When invoked with operands, the exec built-in replaces the currently
//! executing shell process with a new process image, regarding the operands as
//! command words to start the external utility. The first operand identifies
//! the utility, and the other operands are passed to the utility as
//! command-line arguments.
//!
//! Without operands, the built-in does not start any utility. Instead, it makes
//! any redirections performed in the calling simple command permanent in the
//! current shell environment.
//!
//! # Options
//!
//! POSIX defines no options for the exec built-in.
//!
//! The following non-portable options are yet to be implemented:
//!
//! - `--as`
//! - `--clear`
//! - `--cloexec`
//! - `--force`
//! - `--help`
//!
//! # Operands
//!
//! The operands are treated as a command to start an external utility.
//! If any operands are given, the first is the utility name, and the others are
//! its arguments.
//!
//! If the utility name contains a slash character, the shell will treat it as a
//! path to the utility.
//! Otherwise, the shell will search `$PATH` for the utility.
//!
//! # Exit status
//!
//! If the external utility is invoked successfully, it replaces the shell
//! executing the built-in, so there is no exit status of the built-in.
//! If the built-in fails to invoke the utility, the exit status will be 126.
//! If there is no utility matching the first operand, the exit status will be
//! 127.
//!
//! If no operands are given, the exit status will be 0.
//!
//! # Portability
//!
//! POSIX does not require the exec built-in to conform to the Utility Syntax
//! Guidelines, which means portable scripts cannot use any options or the `--`
//! separator for the built-in.
//!
//! # Implementation notes
//!
//! This implementation uses [`Result::retain_redirs`] to flag redirections to
//! be made permanent.
//!
//! If an operand is given and the utility cannot be invoked successfully, the
//! built-in returns a [`Result`] having `Divert::Exit` to request the calling
//! shell to exit. This behavior is not explicitly required by POSIX, but it is
//! a common practice among existing shells.

use std::ffi::CString;
use std::ops::ControlFlow::Break;
use yash_env::builtin::Result;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_semantics::command::simple_command::{replace_current_process, to_c_strings};
use yash_semantics::command_search::search_path;
use yash_semantics::Divert::Abort;
use yash_semantics::ExitStatus;

// TODO Split into syntax and semantics submodules

/// Entry point for executing the `exec` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO Support non-POSIX options
    let mut result = Result::default();
    result.retain_redirs();

    if let Some(name) = args.first() {
        result.set_divert(Break(Abort(None)));

        let path = if name.value.contains('/') {
            CString::new(name.value.clone()).unwrap_or_default()
        } else {
            match search_path(env, name.value.as_str()) {
                Some(path) => path,
                None => {
                    result.set_exit_status(ExitStatus::NOT_FOUND);
                    return result;
                }
            }
        };
        let location = name.origin.clone();
        let args = to_c_strings(args);
        replace_current_process(env, path, args, location).await;
        result.set_exit_status(env.exit_status);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use std::rc::Rc;
    use yash_env::system::r#virtual::{FileBody, INode};
    use yash_env::variable::{Scope, Variable};
    use yash_env::VirtualSystem;

    #[test]
    fn retains_redirs_without_args() {
        let mut env = Env::new_virtual();
        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result.exit_status(), ExitStatus::SUCCESS);
        assert!(result.should_retain_redirs());
    }

    #[test]
    fn executes_external_utility_when_given_operand() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));

        // Prepare the external utility file
        let mut content = INode::default();
        content.body = FileBody::Regular {
            content: Vec::new(),
            is_native_executable: true,
        };
        content.permissions.0 |= 0o100;
        let content = Rc::new(RefCell::new(content));
        system
            .state
            .borrow_mut()
            .file_system
            .save("/bin/echo", content)
            .unwrap();

        // Prepare the PATH variable
        env.variables
            .assign(
                Scope::Global,
                "PATH".to_string(),
                Variable::new("/bin").export(),
            )
            .unwrap();

        let args = Field::dummies(["echo"]);
        _ = main(&mut env, args).now_or_never().unwrap();

        let process = &system.current_process();
        let arguments = process.last_exec().as_ref().unwrap();
        assert_eq!(arguments.0, CString::new("/bin/echo").unwrap());
        assert_eq!(arguments.1, [CString::new("echo").unwrap()]);
        assert_eq!(arguments.2, [CString::new("PATH=/bin").unwrap()]);
    }

    #[test]
    fn passing_argument_to_external_utility() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));

        // Prepare the external utility file
        let mut content = INode::default();
        content.body = FileBody::Regular {
            content: Vec::new(),
            is_native_executable: true,
        };
        content.permissions.0 |= 0o100;
        let content = Rc::new(RefCell::new(content));
        system
            .state
            .borrow_mut()
            .file_system
            .save("/usr/bin/ls", content)
            .unwrap();

        // Prepare the PATH variable
        env.variables
            .assign(
                Scope::Global,
                "PATH".to_string(),
                Variable::new("/usr/bin").export(),
            )
            .unwrap();

        let args = Field::dummies(["ls", "-l"]);
        _ = main(&mut env, args).now_or_never().unwrap();

        let process = &system.current_process();
        let arguments = process.last_exec().as_ref().unwrap();
        assert_eq!(arguments.0, CString::new("/usr/bin/ls").unwrap());
        assert_eq!(
            arguments.1,
            [CString::new("ls").unwrap(), CString::new("-l").unwrap()]
        );
        assert_eq!(arguments.2, [CString::new("PATH=/usr/bin").unwrap()]);
    }

    #[test]
    fn utility_name_with_slash() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));

        // Prepare the external utility file
        let mut content = INode::default();
        content.body = FileBody::Regular {
            content: Vec::new(),
            is_native_executable: true,
        };
        content.permissions.0 |= 0o100;
        let content = Rc::new(RefCell::new(content));
        system
            .state
            .borrow_mut()
            .file_system
            .save("/bin/echo", content)
            .unwrap();

        let args = Field::dummies(["/bin/echo"]);
        _ = main(&mut env, args).now_or_never().unwrap();

        let process = &system.current_process();
        let arguments = process.last_exec().as_ref().unwrap();
        assert_eq!(arguments.0, CString::new("/bin/echo").unwrap());
        assert_eq!(arguments.1, [CString::new("/bin/echo").unwrap()]);
        assert_eq!(arguments.2, []);
    }

    #[test]
    fn utility_not_found() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));

        // Prepare the external utility file
        let mut content = INode::default();
        content.body = FileBody::Regular {
            content: Vec::new(),
            is_native_executable: true,
        };
        content.permissions.0 |= 0o100;
        let content = Rc::new(RefCell::new(content));
        system
            .state
            .borrow_mut()
            .file_system
            .save("/bin/echo", content)
            .unwrap();

        // No PATH variable

        let args = Field::dummies(["echo"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result.exit_status(), ExitStatus::NOT_FOUND);
        assert_eq!(result.divert(), Break(Abort(None)));

        let process = &system.current_process();
        assert_eq!(process.last_exec(), &None);
    }

    #[test]
    fn utility_not_executable() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));

        // Prepare the file without executable permission
        let mut content = INode::default();
        content.body = FileBody::Regular {
            content: Vec::new(),
            is_native_executable: true,
        };
        // content.permissions.0 |= 0o100;
        let content = Rc::new(RefCell::new(content));
        system
            .state
            .borrow_mut()
            .file_system
            .save("/bin/echo", content)
            .unwrap();

        let args = Field::dummies(["/bin/echo"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result.exit_status(), ExitStatus::NOEXEC);
        assert_eq!(result.divert(), Break(Abort(None)));
    }
}
