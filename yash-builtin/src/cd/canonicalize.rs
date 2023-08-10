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

//! Part of the cd built-in that canonicalizes the target directory path

use std::ffi::CString;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::path::PathBuf;
use thiserror::Error;
use yash_env::System;

#[derive(Debug, Clone, Eq, Error, PartialEq)]
#[error("non-existing directory component '{}'", missing.display())]
pub struct NonExistingDirectoryError {
    /// Path to the non-existing directory
    pub missing: PathBuf,
}

/// Canonicalizes the target directory path.
///
/// - Removes dot components.
/// - Removes dot-dot components along with the preceding component.
/// - Removes redundant slashes.
///
/// It is an error if a component preceding a dot-dot component refers to a
/// non-existent directory. In other words, in order to canonicalize "a/b/../c"
/// into "a/c", the directory "a/b" must exist.
pub fn canonicalize<S: System>(
    system: &S,
    path: &Path,
) -> Result<PathBuf, NonExistingDirectoryError> {
    let path = path.as_os_str().as_bytes();
    let leading_slashes = path.iter().take_while(|&&b| b == b'/').count();
    let mut components = path
        .split(|&b| b == b'/')
        // Filtering out empty components means removing redundant slashes.
        // We also remove dot components here.
        .filter(|&c| !c.is_empty() && c != b".")
        .collect::<Vec<_>>();
    remove_dot_dot(system, leading_slashes, &mut components)?;
    Ok(create_path(leading_slashes, &components))
}

/// Removes dot-dot components along with the preceding component.
fn remove_dot_dot<S: System>(
    system: &S,
    leading_slashes: usize,
    components: &mut Vec<&[u8]>,
) -> Result<(), NonExistingDirectoryError> {
    let mut index = 1;
    while let Some(&component) = components.get(index) {
        if component != b".." {
            index += 1;
            continue;
        }
        if components[index - 1] == b".." {
            // We have two consecutive dot-dot components, first of which was
            // not removed in the previous iteration. This means we have
            // consecutive dot-dot components as in "/../..".  Removing these
            // components would render an incorrect result.
            index += 1;
            continue;
        }

        // Check if the parent directory exists
        let parent = create_path(leading_slashes, &components[..index]);
        ensure_directory(system, parent)?;

        // Do remove the dot-dot and the preceding component
        components.drain(index - 1..index + 1);
        if index > 1 {
            index -= 1;
        }
    }
    Ok(())
}

fn create_path(leading_slashes: usize, components: &[&[u8]]) -> PathBuf {
    let mut result = OsString::new();

    match leading_slashes {
        0 => {}
        2 => result.push("//"),
        _ => result.push("/"),
    }

    for component in components {
        if !result.is_empty() && !result.as_bytes().ends_with(b"/") {
            result.push("/");
        }
        result.push(OsStr::from_bytes(component));
    }

    result.into()
}

/// Returns an error if the given path is not a directory.
fn ensure_directory<S: System>(system: &S, path: PathBuf) -> Result<(), NonExistingDirectoryError> {
    match CString::new(path.into_os_string().into_vec()) {
        Ok(path) if system.is_directory(&path) => Ok(()),
        Ok(path) => Err(NonExistingDirectoryError {
            missing: OsString::from_vec(path.into_bytes()).into(),
        }),
        Err(e) => Err(NonExistingDirectoryError {
            missing: OsString::from_vec(e.into_vec()).into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::system::r#virtual::INode;
    use yash_env::system::r#virtual::VirtualSystem;

    #[test]
    fn empty_path() {
        let system = VirtualSystem::new();
        let result = canonicalize(&system, Path::new("")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok(""));
    }

    #[test]
    fn single_slash_root() {
        let system = VirtualSystem::new();
        let result = canonicalize(&system, Path::new("/")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/"));
    }

    #[test]
    fn double_slash_root() {
        let system = VirtualSystem::new();
        let result = canonicalize(&system, Path::new("//")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("//"));
    }

    #[test]
    fn triple_slash_root() {
        let system = VirtualSystem::new();
        let result = canonicalize(&system, Path::new("///")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/"));
    }

    #[test]
    fn rootless_non_empty() {
        let system = VirtualSystem::new();
        let result = canonicalize(&system, Path::new("foo/bar")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("foo/bar"));
    }

    #[test]
    fn single_component() {
        let system = VirtualSystem::new();
        let result = canonicalize(&system, Path::new("/home")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/home"));
    }

    #[test]
    fn double_component() {
        let system = VirtualSystem::new();
        let result = canonicalize(&system, Path::new("/home/user")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/home/user"));
    }

    #[test]
    fn many_components() {
        let system = VirtualSystem::new();
        let result = canonicalize(&system, Path::new("///usr/local/share/yash")).unwrap();
        assert_eq!(
            from_utf8(result.as_os_str().as_bytes()),
            Ok("/usr/local/share/yash")
        );
    }

    #[test]
    fn redundant_slashes() {
        let system = VirtualSystem::new();
        let result = canonicalize(&system, Path::new("///usr//local///share//yash")).unwrap();
        assert_eq!(
            from_utf8(result.as_os_str().as_bytes()),
            Ok("/usr/local/share/yash")
        );
    }

    #[test]
    fn trailing_slashes() {
        let system = VirtualSystem::new();

        let result = canonicalize(&system, Path::new("/foo/")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/foo"));

        let result = canonicalize(&system, Path::new("/foo/bar//")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/foo/bar"));
    }

    #[test]
    fn dot() {
        let system = VirtualSystem::new();

        let result = canonicalize(&system, Path::new("/usr/./local/share/./yash")).unwrap();
        assert_eq!(
            from_utf8(result.as_os_str().as_bytes()),
            Ok("/usr/local/share/yash")
        );

        let result = canonicalize(&system, Path::new("/./")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/"));

        let result = canonicalize(&system, Path::new("//./")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("//"));

        let result = canonicalize(&system, Path::new("/foo/.")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/foo"));
    }

    #[test]
    fn dot_dot_with_existing_directories() {
        let system = VirtualSystem::new();
        system
            .state
            .borrow_mut()
            .file_system
            .save("/foo/bar/file", Rc::new(INode::default().into()))
            .unwrap();

        // Components AFTER the dot-dot do not have to exist.
        let result = canonicalize(&system, Path::new("/foo/bar/../baz")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/foo/baz"));

        let result = canonicalize(&system, Path::new("/foo/../bar/baz")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/bar/baz"));
    }

    #[test]
    fn double_dot_dot() {
        let system = VirtualSystem::new();
        system
            .state
            .borrow_mut()
            .file_system
            .save("/foo/bar/file", Rc::new(INode::default().into()))
            .unwrap();

        let result = canonicalize(&system, Path::new("/foo/bar/../../baz")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/baz"));
    }

    #[test]
    fn dot_dot_after_dot() {
        let system = VirtualSystem::new();
        system
            .state
            .borrow_mut()
            .file_system
            .save("/foo/bar/file", Rc::new(INode::default().into()))
            .unwrap();

        let result = canonicalize(&system, Path::new("/foo/bar/./../baz")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/foo/baz"));
    }

    #[test]
    fn dot_dot_after_root() {
        // "/.." should not become "/"
        let system = VirtualSystem::new();
        let result = canonicalize(&system, Path::new("/..")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/.."));

        let result = canonicalize(&system, Path::new("/../..")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/../.."));

        let result = canonicalize(&system, Path::new("/../../..")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/../../.."));
    }

    #[test]
    fn dot_dot_with_symlink() {
        let system = VirtualSystem::new();
        let symlink = INode {
            body: yash_env::system::r#virtual::FileBody::Symlink {
                target: PathBuf::from("."),
            },
            permissions: Default::default(),
        };
        system
            .state
            .borrow_mut()
            .file_system
            .save("/foo/bar/link", Rc::new(symlink.into()))
            .unwrap();

        let result = canonicalize(&system, Path::new("/foo/bar/link/../baz")).unwrap();
        assert_eq!(from_utf8(result.as_os_str().as_bytes()), Ok("/foo/bar/baz"));
    }

    #[test]
    fn dot_dot_with_non_existing_directory() {
        let system = VirtualSystem::new();

        let e = canonicalize(&system, Path::new("/foo/bar/../baz")).unwrap_err();
        assert_eq!(e.missing, Path::new("/foo/bar"));

        let e = canonicalize(&system, Path::new("/foo/../bar/baz")).unwrap_err();
        assert_eq!(e.missing, Path::new("/foo"));
    }
}
