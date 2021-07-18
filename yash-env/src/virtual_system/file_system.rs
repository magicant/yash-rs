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

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

/// Collection of files.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileSystem(HashMap<PathBuf, Rc<RefCell<INode>>>);
// TODO should be a link to the root i-node
// In the current implementation, this hash map stores all files in a flat
// namespace, without any recursive directory structure.

impl FileSystem {
    /// Saves a file.
    ///
    /// If there is an existing file at the specified path, it is replaced with
    /// the new file and returned.
    pub fn save(
        &mut self,
        path: PathBuf,
        content: Rc<RefCell<INode>>,
    ) -> Option<Rc<RefCell<INode>>> {
        self.0.insert(path, content)
    }

    /// Returns a reference to the existing file at the specified path.
    pub fn get(&self, path: &Path) -> Option<&Rc<RefCell<INode>>> {
        // TODO Return ENOTDIR or ENOENT if not found
        self.0.get(path)
    }
}

/// File on the file system.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct INode {
    /// File content.
    pub content: Vec<u8>,
    /// Access permissions.
    pub permissions: Mode,
    /// Whether this file is a native binary that can be exec'ed.
    pub is_native_executable: bool,
    // TODO owner user and group, etc.
}

impl INode {
    /// Create an empty regular file.
    pub fn new() -> INode {
        INode::default()
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
