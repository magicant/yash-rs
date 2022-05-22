// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! File system in a virtual system.

use nix::errno::Errno;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::path::Component;
use std::path::Path;
use std::rc::Rc;

/// Collection of files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileSystem {
    /// Root directory
    pub root: Rc<RefCell<INode>>,
}

/// The default file system only contains an empty root directory.
impl Default for FileSystem {
    fn default() -> Self {
        FileSystem {
            root: Rc::new(RefCell::new(INode {
                body: FileBody::Directory {
                    files: HashMap::new(),
                },
                permissions: Mode(0o755),
            })),
        }
    }
}

impl FileSystem {
    /// Saves a file.
    ///
    /// If there is an existing file at the specified path, this function
    /// replaces it the new file and returns the old one, regardless of
    /// permissions.
    ///
    /// TODO Reject relative path
    pub fn save<P: AsRef<Path>>(
        &mut self,
        path: P,
        content: Rc<RefCell<INode>>,
    ) -> nix::Result<Option<Rc<RefCell<INode>>>> {
        fn ensure_dir(body: &mut FileBody) -> &mut HashMap<Box<OsStr>, Rc<RefCell<INode>>> {
            match body {
                FileBody::Directory { files } => files,
                _ => {
                    let files = HashMap::new();
                    *body = FileBody::Directory { files };
                    match body {
                        FileBody::Directory { files } => files,
                        _ => unreachable!(),
                    }
                }
            }
        }

        fn main(
            fs: &mut FileSystem,
            path: &Path,
            content: Rc<RefCell<INode>>,
        ) -> nix::Result<Option<Rc<RefCell<INode>>>> {
            let mut components = path.components();
            let file_name = match components.next_back().ok_or(Errno::ENOENT)? {
                Component::Normal(name) => name,
                _ => return Err(Errno::ENOENT),
            };

            // Create parent directories
            let mut node = Rc::clone(&fs.root);
            for component in components {
                let name = match component {
                    Component::Normal(name) => name,
                    Component::RootDir => continue,
                    _ => return Err(Errno::ENOENT),
                };
                let mut node_ref = node.borrow_mut();
                let children = ensure_dir(&mut node_ref.body);
                use std::collections::hash_map::Entry::*;
                let child = match children.entry(Box::from(name)) {
                    Occupied(occupied) => Rc::clone(occupied.get()),
                    Vacant(vacant) => {
                        let child = Rc::new(RefCell::new(INode {
                            body: FileBody::Directory {
                                files: HashMap::new(),
                            },
                            permissions: Mode(0o755),
                        }));
                        Rc::clone(vacant.insert(child))
                    }
                };
                drop(node_ref);
                node = child;
            }

            let mut parent_ref = node.borrow_mut();
            let children = ensure_dir(&mut parent_ref.body);
            Ok(children.insert(Box::from(file_name), content))
        }

        main(self, path.as_ref(), content)
    }

    /// Returns a reference to the existing file at the specified path.
    ///
    /// TODO Reject relative path
    pub fn get<P: AsRef<Path>>(&self, path: P) -> nix::Result<Rc<RefCell<INode>>> {
        fn main(fs: &FileSystem, path: &Path) -> nix::Result<Rc<RefCell<INode>>> {
            let components = path.components();
            let mut node = Rc::clone(&fs.root);
            for component in components {
                let name = match component {
                    Component::Normal(name) => name,
                    Component::RootDir => continue,
                    _ => return Err(Errno::ENOENT),
                };
                let node_ref = node.borrow();
                let children = match &node_ref.body {
                    FileBody::Directory { files } => files,
                    _ => return Err(Errno::ENOTDIR),
                };
                let child = Rc::clone(children.get(name).ok_or(Errno::ENOENT)?);
                drop(node_ref);
                node = child;
            }
            Ok(node)
        }

        main(self, path.as_ref())
    }
}

/// File on the file system.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct INode {
    /// File content.
    pub body: FileBody,
    /// Access permissions.
    pub permissions: Mode,
    // TODO owner user and group, etc.
}

impl INode {
    /// Create a regular file with the given content.
    pub fn new<T: Into<Vec<u8>>>(bytes: T) -> Self {
        INode {
            body: FileBody::new(bytes),
            permissions: Mode::default(),
        }
    }
}

/// Filetype-specific content of a file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileBody {
    /// Regular file
    Regular {
        /// File content.
        content: Vec<u8>,
        /// Whether this file is a native binary that can be exec'ed.
        is_native_executable: bool,
    },
    Directory {
        /// Files contained in this directory.
        ///
        /// The keys of the hashmap are filenames without any parent directory
        /// components. The hashmap does not contain "." or "..".
        files: HashMap<Box<OsStr>, Rc<RefCell<INode>>>,
    },
    // TODO Other filetypes
}

/// The default file body is an empty regular file.
impl Default for FileBody {
    fn default() -> Self {
        FileBody::Regular {
            content: Vec::default(),
            is_native_executable: bool::default(),
        }
    }
}

impl FileBody {
    /// Creates a regular file body with the given content.
    pub fn new<T: Into<Vec<u8>>>(bytes: T) -> Self {
        FileBody::Regular {
            content: bytes.into(),
            is_native_executable: false,
        }
    }
}

/// File permission bits.
///
/// The `Default` mode is `0o644`, not `0o000`.
#[derive(Copy, Clone, Eq, Hash, PartialEq)]
pub struct Mode(pub u32);

impl Debug for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mode({:#o})", self.0)
    }
}

impl Default for Mode {
    fn default() -> Mode {
        Mode(0o644)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn file_system_get_root() {
        let fs = FileSystem::default();
        let result = fs.get("/");
        assert_eq!(result, Ok(fs.root));
    }

    #[test]
    fn file_system_save_and_get_file() {
        let mut fs = FileSystem::default();
        let file_1 = Rc::new(RefCell::new(INode::new([12, 34, 56])));
        let old = fs.save("/foo/bar", Rc::clone(&file_1));
        assert_eq!(old, Ok(None));

        let file_2 = Rc::new(RefCell::new(INode::new([98, 76, 54])));
        let old = fs.save("/foo/bar", Rc::clone(&file_2));
        assert_eq!(old, Ok(Some(file_1)));

        let result = fs.get("/foo/bar");
        assert_eq!(result, Ok(file_2));
    }

    #[test]
    fn file_system_save_and_get_directory() {
        let mut fs = FileSystem::default();
        let file = Rc::new(RefCell::new(INode::new([12, 34, 56])));
        let old = fs.save("/foo/bar", Rc::clone(&file));
        assert_eq!(old, Ok(None));

        let dir = fs.get("/foo").unwrap();
        let dir = dir.borrow();
        assert_eq!(dir.permissions, Mode(0o755));
        assert_matches!(&dir.body, FileBody::Directory { files } => {
            let mut i = files.iter();
            let (name, content) = i.next().unwrap();
            assert_eq!(name.as_ref(), Path::new("bar"));
            assert_eq!(content, &file);
            assert_eq!(i.next(), None);
        });
    }

    #[test]
    fn file_system_save_invalid_name() {
        let mut fs = FileSystem::default();
        let old = fs.save("", Rc::default());
        assert_eq!(old, Err(Errno::ENOENT));
    }

    #[test]
    fn file_system_get_non_existent_file() {
        let fs = FileSystem::default();
        let result = fs.get("/no_such_file");
        assert_eq!(result, Err(Errno::ENOENT));
        let result = fs.get("/no_such_directory/foo");
        assert_eq!(result, Err(Errno::ENOENT));
    }

    #[test]
    fn file_system_get_not_directory() {
        let mut fs = FileSystem::default();
        let _ = fs.save("/file", Rc::default());
        let result = fs.get("/file/foo");
        assert_eq!(result, Err(Errno::ENOTDIR));
    }
}
