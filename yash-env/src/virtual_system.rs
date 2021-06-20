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
use std::ffi::CStr;
use std::fmt::Debug;
use std::path::PathBuf;

/// Simulated system.
///
/// See the [module-level documentation](self) to grasp a basic understanding of
/// `VirtualSystem`.
///
/// The `Clone` implementation for `VirtualSystem` creates an entire copy that
/// works independently of the original.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualSystem {}

impl VirtualSystem {
    /// Creates a virtual system with an empty state.
    pub fn new() -> VirtualSystem {
        VirtualSystem {}
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

    fn is_executable_file(&self, _: &CStr) -> bool {
        todo!()
    }
}

/// Collection of files.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileSystem(pub HashMap<PathBuf, INode>);
// TODO should be a link to the root i-node
// In the current implementation, this hash map stores all files in a flat
// namespace, without any recursive directory structure.

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
