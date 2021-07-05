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

//! System simulated in Rust.
//!
//! [`VirtualSystem`] is a pure Rust implementation of [`System`] that simulates
//! the behavior of the underlying system without any interaction with the
//! actual system. `VirtualSystem` is used for testing the behavior of the shell
//! in unit tests.
//!
//! This module also defines elements that compose a virtual system.

use crate::System;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::ffi::CStr;
use std::ffi::CString;
use std::fmt::Debug;
use std::path::Path;
use std::path::PathBuf;

// TODO VirtualSystem is not PartialEq because ForkResult is not.
/// Simulated system.
///
/// See the [module-level documentation](self) to grasp a basic understanding of
/// `VirtualSystem`.
///
/// The `Clone` implementation for `VirtualSystem` creates an entire copy that
/// works independently of the original.
#[derive(Clone, Debug)]
pub struct VirtualSystem {
    /// Collection of files existing in the virtual system.
    pub file_system: FileSystem,

    /// Results of future calls to [`fork`](Self::fork).
    pub pending_forks: VecDeque<nix::Result<nix::unistd::ForkResult>>,

    /// Results of future calls to [`wait`](Self::wait).
    pub pending_waits: VecDeque<nix::Result<nix::sys::wait::WaitStatus>>,
}

impl VirtualSystem {
    /// Creates a virtual system with an empty state.
    pub fn new() -> VirtualSystem {
        VirtualSystem {
            file_system: FileSystem::default(),
            pending_forks: Default::default(),
            pending_waits: Default::default(),
        }
    }
}

impl Default for VirtualSystem {
    /// Creates a virtual system with a sensible default state.
    fn default() -> VirtualSystem {
        VirtualSystem::new()
    }
}

impl System for VirtualSystem {
    fn clone_box(&self) -> Box<dyn System> {
        Box::new(self.clone())
    }

    /// Tests whether the specified file is executable or not.
    ///
    /// The current implementation only checks if the file has any executable
    /// bit in the permissions. The file owner and group are not considered.
    fn is_executable_file(&self, path: &CStr) -> bool {
        let path = match path.to_str() {
            Ok(path) => PathBuf::from(path),
            Err(_) => return false,
        };
        match self.file_system.get(&path) {
            None => false,
            Some(inode) => inode.permissions.0 & 0o111 != 0,
        }
    }

    /// Simulates cloning the current shell process.
    ///
    /// This implementation pops the first entry from
    /// [`pending_forks`](VirtualSystem::pending_forks) and returns it.
    /// If `pending_forks` is empty, this function will **panic**!
    ///
    /// # Safety
    ///
    /// Though [`System::fork`] is declared `unsafe`, this implementation of
    /// `fork` is safe.
    unsafe fn fork(&mut self) -> nix::Result<nix::unistd::ForkResult> {
        self.pending_forks
            .pop_front()
            .expect("pending_forks must be filled before calling fork")
    }

    /// Simulates awaiting child process status update.
    ///
    /// This implementation pops the first entry from
    /// [`pending_waits`](VirtualSystem::pending_waits) and returns it.
    /// If `pending_waits` is empty, this function will **panic**!
    fn wait(&mut self) -> nix::Result<nix::sys::wait::WaitStatus> {
        self.pending_waits
            .pop_front()
            .expect("pending_waits must be filled before calling wait")
    }

    /// **Panic!**
    ///
    /// The `execve` system call cannot be simulated in the userland. This
    /// function always panics.
    fn execve(
        &mut self,
        path: &CStr,
        args: &[CString],
        _envs: &[CString],
    ) -> nix::Result<Infallible> {
        panic!(
            "VirtualSystem::execve called for path={:?}, args={:?}",
            path, args
        );
    }
}

/// Collection of files.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileSystem(HashMap<PathBuf, INode>);
// TODO should be a link to the root i-node
// In the current implementation, this hash map stores all files in a flat
// namespace, without any recursive directory structure.

impl FileSystem {
    /// Saves a file.
    ///
    /// If there is an existing file at the specified path, it is replaced with
    /// the new file and returned.
    pub fn save(&mut self, path: PathBuf, content: INode) -> Option<INode> {
        self.0.insert(path, content)
    }

    /// Returns a reference to the existing file at the specified path.
    pub fn get(&self, path: &Path) -> Option<&INode> {
        self.0.get(path)
    }
}

/// File on the file system.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct INode {
    pub permissions: Mode,
    // TODO File content, owner user and group, etc.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn is_executable_file_non_existing_file() {
        let system = VirtualSystem::new();
        assert!(!system.is_executable_file(&CString::new("/no/such/file").unwrap()));
    }

    #[test]
    fn is_executable_file_existing_but_non_executable_file() {
        let mut system = VirtualSystem::new();
        let path = PathBuf::from("/some/file");
        let content = INode::default();
        system.file_system.save(path, content);
        assert!(!system.is_executable_file(&CString::new("/some/file").unwrap()));
    }

    #[test]
    fn is_executable_file_with_executable_file() {
        let mut system = VirtualSystem::new();
        let path = PathBuf::from("/some/file");
        let mut content = INode::default();
        content.permissions.0 |= 0o100;
        system.file_system.save(path, content);
        assert!(system.is_executable_file(&CString::new("/some/file").unwrap()));
    }
}
