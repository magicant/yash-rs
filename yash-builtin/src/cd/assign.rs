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

//! Part of the cd built-in that updates `$PWD` and `$OLDPWD`

use super::Mode;
use crate::common::arrange_message_and_divert;
use yash_env::path::Path;
use yash_env::path::PathBuf;
use yash_env::variable::AssignError;
use yash_env::variable::Scope::Global;
use yash_env::variable::Value::Scalar;
use yash_env::variable::OLDPWD;
use yash_env::variable::PWD;
use yash_env::Env;
use yash_env::System;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;

/// Assigns the given value to `$OLDPWD`.
///
/// If `$OLDPWD` is read-only, this function prints an error message and does
/// nothing.
///
/// This function examines the stack to find the command location that invoked
/// the cd built-in.
pub async fn set_oldpwd(env: &mut Env, value: String) {
    set_variable(env, OLDPWD, value).await
}

/// Assigns the working directory path to `$PWD`.
///
/// This function assigns the given `path` to the `PWD` variable.
/// If `$PWD` is read-only, this function prints an error message and does
/// nothing.
///
/// This function examines the stack to find the command location that invoked
/// the cd built-in.
pub async fn set_pwd(env: &mut Env, path: PathBuf) {
    let value = path.into_unix_string().into_string().unwrap_or_default();
    set_variable(env, PWD, value).await
}

async fn set_variable(env: &mut Env, name: &str, value: String) {
    let current_builtin = env.stack.current_builtin();
    let current_location = current_builtin.map(|builtin| builtin.name.origin.clone());
    let var = &mut env.get_or_create_variable(name, Global);
    match var.assign(Scalar(value), current_location) {
        Ok(_) => {}
        Err(error) => return handle_assign_error(env, name, error).await,
    }
    var.export(true);
}

/// Prints a warning message for a read-only variable.
///
/// The message is only a warning because it does not affect the exit status.
async fn handle_assign_error(env: &mut Env, name: &str, error: AssignError) {
    let message = Message {
        r#type: AnnotationType::Warning,
        title: format!("cannot update read-only variable `{}`", name).into(),
        annotations: vec![Annotation::new(
            AnnotationType::Info,
            "the variable was made read-only here".into(),
            &error.read_only_location,
        )],
        footers: vec![],
    };
    let (message, _divert) = arrange_message_and_divert(env, message);
    env.system.print_error(&message).await;
}

/// Computes the new value of `$PWD`.
///
/// If `mode` is `Logical`, this function returns `path` without any
/// modification. If `mode` is `Physical`, this function uses [`System::getcwd`]
/// to obtain the working directory path.
pub fn new_pwd(env: &Env, mode: Mode, path: &Path) -> PathBuf {
    match mode {
        Mode::Logical => path.to_owned(),
        Mode::Physical => env.system.getcwd().unwrap_or_else(|_| PathBuf::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use yash_env::semantics::Field;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env::VirtualSystem;
    use yash_env_test_helper::{assert_stderr, assert_stdout};
    use yash_syntax::source::Location;

    #[test]
    fn set_oldpwd_new() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let cd = Field::dummy("cd");
        let location = cd.origin.clone();
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: cd,
            is_special: false,
        }));

        set_oldpwd(&mut env, "/some/path".to_string())
            .now_or_never()
            .unwrap();
        let variable = env.variables.get(OLDPWD).unwrap();
        assert_eq!(variable.value, Some(Scalar("/some/path".to_string())));
        assert_eq!(variable.quirk, None);
        assert_eq!(variable.last_assigned_location, Some(location));
        assert!(variable.is_exported);
        assert_eq!(variable.read_only_location, None);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn set_oldpwd_overwrites_existing_variable() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let cd = Field::dummy("cd");
        let location = cd.origin.clone();
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: cd,
            is_special: false,
        }));
        env.get_or_create_variable(OLDPWD, Global)
            .assign("/old/pwd", None)
            .unwrap();

        set_oldpwd(&mut env, "/some/dir".to_string())
            .now_or_never()
            .unwrap();
        let variable = env.variables.get(OLDPWD).unwrap();
        assert_eq!(variable.value, Some(Scalar("/some/dir".to_string())));
        assert_eq!(variable.quirk, None);
        assert_eq!(variable.last_assigned_location, Some(location));
        assert!(variable.is_exported);
        assert_eq!(variable.read_only_location, None);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn set_oldpwd_read_only_error() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("cd"),
            is_special: false,
        }));
        let read_only_location = Location::dummy("read-only");
        let mut oldpwd = env.get_or_create_variable(OLDPWD, Global);
        oldpwd.assign("/old/pwd", None).unwrap();
        oldpwd.make_read_only(read_only_location.clone());

        set_oldpwd(&mut env, "/foo".to_string())
            .now_or_never()
            .unwrap();
        let variable = env.variables.get(OLDPWD).unwrap();
        assert_eq!(variable.value, Some(Scalar("/old/pwd".to_string())));
        assert_eq!(variable.last_assigned_location, None);
        assert_eq!(variable.read_only_location, Some(read_only_location));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn set_pwd_new() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let cd = Field::dummy("cd");
        let location = cd.origin.clone();
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: cd,
            is_special: false,
        }));

        set_pwd(&mut env, PathBuf::from("/some/path"))
            .now_or_never()
            .unwrap();
        let variable = env.variables.get(PWD).unwrap();
        assert_eq!(variable.value, Some(Scalar("/some/path".to_string())));
        assert_eq!(variable.quirk, None);
        assert_eq!(variable.last_assigned_location, Some(location));
        assert!(variable.is_exported);
        assert_eq!(variable.read_only_location, None);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn set_pwd_overwrites_existing_variable() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let cd = Field::dummy("cd");
        let location = cd.origin.clone();
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: cd,
            is_special: false,
        }));
        env.get_or_create_variable(PWD, Global)
            .assign("/old/path", None)
            .unwrap();

        set_pwd(&mut env, PathBuf::from("/some/path"))
            .now_or_never()
            .unwrap();
        let variable = env.variables.get(PWD).unwrap();
        assert_eq!(variable.value, Some(Scalar("/some/path".to_string())));
        assert_eq!(variable.quirk, None);
        assert_eq!(variable.last_assigned_location, Some(location));
        assert!(variable.is_exported);
        assert_eq!(variable.read_only_location, None);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn set_pwd_read_only_error() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let cd = Field::dummy("cd");
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: cd,
            is_special: false,
        }));
        let read_only_location = Location::dummy("read-only");
        let mut pwd = env.get_or_create_variable(PWD, Global);
        pwd.assign("/old/path", None).unwrap();
        pwd.make_read_only(read_only_location.clone());

        set_pwd(&mut env, PathBuf::from("/some/path"))
            .now_or_never()
            .unwrap();
        let variable = env.variables.get(PWD).unwrap();
        assert_eq!(variable.value, Some(Scalar("/old/path".to_string())));
        assert_eq!(variable.last_assigned_location, None);
        assert_eq!(variable.read_only_location, Some(read_only_location));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn new_pwd_physical() {
        let mut system = Box::new(VirtualSystem::new());
        system
            .current_process_mut()
            .chdir(PathBuf::from("/some/path"));
        let env = Env::with_system(system);

        let result = new_pwd(&env, Mode::Physical, Path::new(".."));
        assert_eq!(result, Path::new("/some/path"));
    }

    #[test]
    fn new_pwd_logical() {
        let mut system = Box::new(VirtualSystem::new());
        system
            .current_process_mut()
            .chdir(PathBuf::from("/some/path"));
        let env = Env::with_system(system);

        let result = new_pwd(&env, Mode::Logical, Path::new("/foo/bar"));
        assert_eq!(result, Path::new("/foo/bar"));
    }
}
