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

//! Part of the cd built-in that searches `$CDPATH`

use std::ffi::CString;
use yash_env::path::Path;
use yash_env::path::PathBuf;
use yash_env::str::UnixString;
use yash_env::variable::CDPATH;
use yash_env::Env;
use yash_env::System;

/// Searches `$CDPATH` for the given path.
///
/// This function treats the value of `$CDPATH` as a colon-separated list of
/// directory paths. It finds the first directory that contains the given path
/// as a subdirectory and returns the subdirectory path created by concatenating
/// the directory path and the given path. If no such directory is found, the
/// return value is `None`, in which case the given path should be used as is.
///
/// If a path in `$CDPATH` is empty, the current working directory is used. In
/// this case, if the given path names a directory, no more directories are
/// searched, but the return value is `None`.
///
/// If `$CDPATH` is an array, each element is treated as a directory path.
///
/// If the given path is absolute or starts with a "." or ".." component, this
/// function just returns `None` without any search.
pub fn search(env: &Env, path: &Path) -> Option<PathBuf> {
    if path.is_absolute() || path.starts_with(".") || path.starts_with("..") {
        return None;
    }

    for base in env.variables.get(CDPATH)?.value.as_ref()?.split() {
        let full_path = Path::new(base).join(path);
        // TODO The current Rust implementation joins "//" and "foo" into "/foo"
        // where "//foo" is expected, but Rust is not yet ported to platforms
        // where this difference matters. We may need to revisit this when Rust
        // supports such a platform, notably Cygwin.

        if let Some(full_path) = ensure_directory(&env.system, full_path) {
            return Some(full_path).filter(|_| !base.is_empty());
        }
    }

    None
}

/// Checks if the given path is a directory and returns it if so.
///
/// This function requires the ownership of the given path to create a temporary
/// `CString` used in the underlying system call.
fn ensure_directory<S: System>(system: &S, path: PathBuf) -> Option<PathBuf> {
    match CString::new(path.into_unix_string().into_vec()) {
        Ok(path) if system.is_directory(&path) => {
            Some(UnixString::from_vec(path.into_bytes()).into())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use yash_env::system::r#virtual::Inode;
    use yash_env::variable::Scope::Global;
    use yash_env::variable::Value;
    use yash_env::VirtualSystem;

    fn create_dummy_file(system: &VirtualSystem, path: &str) {
        system
            .state
            .borrow_mut()
            .file_system
            .save(path, Rc::new(Inode::default().into()))
            .unwrap();
    }

    #[test]
    fn unset_cdpath() {
        let env = Env::new_virtual();
        assert_eq!(search(&env, Path::new("foo")), None);
        assert_eq!(search(&env, Path::new("/bar")), None);
    }

    #[test]
    fn directory_not_found_from_cdpath() {
        let mut env = Env::new_virtual();
        env.get_or_create_variable(CDPATH, Global)
            .assign("/foo:/bar", None)
            .unwrap();
        assert_eq!(search(&env, Path::new("one")), None);
        assert_eq!(search(&env, Path::new("/two")), None);
    }

    #[test]
    fn directory_found_from_scalar_cdpath() {
        let system = Box::new(VirtualSystem::new());
        create_dummy_file(&system, "/foo/one/file");
        create_dummy_file(&system, "/bar/two/file");
        let mut env = Env::with_system(system);
        env.get_or_create_variable(CDPATH, Global)
            .assign("/foo:/bar:/x", None)
            .unwrap();

        assert_eq!(
            search(&env, Path::new("one")),
            Some(PathBuf::from("/foo/one")),
        );
        assert_eq!(
            search(&env, Path::new("two")),
            Some(PathBuf::from("/bar/two")),
        );
    }

    #[test]
    fn directory_found_from_array_cdpath() {
        let system = Box::new(VirtualSystem::new());
        create_dummy_file(&system, "/foo/one/file");
        create_dummy_file(&system, "/bar/two/file");
        let mut env = Env::with_system(system);
        env.get_or_create_variable(CDPATH, Global)
            .assign(Value::array(["/foo", "/bar", "/x"]), None)
            .unwrap();

        assert_eq!(
            search(&env, Path::new("one")),
            Some(PathBuf::from("/foo/one")),
        );
        assert_eq!(
            search(&env, Path::new("two")),
            Some(PathBuf::from("/bar/two")),
        );
    }

    #[test]
    fn empty_directory_name_in_cdpath() {
        let mut system = Box::new(VirtualSystem::new());
        create_dummy_file(&system, "/foo/one/file");
        create_dummy_file(&system, "/bar/two/file");
        system.current_process_mut().chdir("/bar".into());
        let mut env = Env::with_system(system);
        env.get_or_create_variable(CDPATH, Global)
            .assign("/foo::/baz", None)
            .unwrap();

        assert_eq!(search(&env, Path::new("two")), None);
    }

    #[test]
    fn path_starting_with_dot() {
        let mut system = Box::new(VirtualSystem::new());
        create_dummy_file(&system, "/foo/one/file");
        create_dummy_file(&system, "/bar/two/file");
        system.current_process_mut().chdir("/".into());
        let mut env = Env::with_system(system);
        env.get_or_create_variable(CDPATH, Global)
            .assign("/foo:/bar:/x", None)
            .unwrap();

        assert_eq!(search(&env, Path::new("./one")), None);
    }

    #[test]
    fn path_starting_with_dot_dot() {
        let mut system = Box::new(VirtualSystem::new());
        create_dummy_file(&system, "/foo/one/file");
        create_dummy_file(&system, "/bar/two/file");
        system.current_process_mut().chdir("/bar/two".into());
        let mut env = Env::with_system(system);
        env.get_or_create_variable(CDPATH, Global)
            .assign("/foo:/bar:/x", None)
            .unwrap();

        assert_eq!(search(&env, Path::new("../foo")), None);
    }

    #[test]
    fn absolute_path() {
        let system = Box::new(VirtualSystem::new());
        create_dummy_file(&system, "/foo/one/file");
        let mut env = Env::with_system(system);
        env.get_or_create_variable(CDPATH, Global)
            .assign("/foo:/bar:/x", None)
            .unwrap();

        assert_eq!(search(&env, Path::new("/foo")), None);
    }
}
