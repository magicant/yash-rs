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

//! Implementation of `System` that actually interacts with the system.

use super::System;
use nix::unistd::access;
use nix::unistd::AccessFlags;
use std::ffi::CStr;

/// Implementation of `System` that actually interacts with the system.
///
/// `RealSystem` has no state at the Rust level because the relevant state of
/// the environment is managed by the underlying operating system.
///
/// # Cloning semantics
///
/// Although this struct implements `System::clone_box`, the state of the
/// underlying system cannot be cloned. It just returns another `Box` of
/// `RealSystem`. Having more than one instance of `RealSystem` to manipulate
/// the system concurrently is not a good idea since all the `RealSystem`s
/// interact with one and the same system.
///
/// # Feature availability
///
/// `RealSystem` is available by default, but can be excluded by disabling the
/// `real-system` feature of the `yash-env` crate. This would remove dependency
/// on the `nix` crate.
#[derive(Debug)]
pub struct RealSystem;

impl System for RealSystem {
    /// Returns `RealSystem` in a new box.
    ///
    /// See the [documentation for the struct](RealSystem) for the implications
    /// of cloning `RealSystem`.
    fn clone_box(&self) -> Box<dyn System> {
        Box::new(RealSystem)
    }

    fn is_executable_file(&self, path: &CStr) -> bool {
        let flags = AccessFlags::X_OK;
        access(path, flags).is_ok()
        // TODO Should use eaccess
    }
}
