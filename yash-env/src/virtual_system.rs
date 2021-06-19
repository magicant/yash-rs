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

use crate::System;
use std::ffi::CStr;

/// Simulated system.
///
/// See the [module-level documentation](self) to grasp a basic understanding of
/// `VirtualSystem`.
///
/// The `Clone` implementation for `VirtualSystem` creates an entire copy that
/// works independently of the original.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualSystem {}

impl System for VirtualSystem {
    fn clone_box(&self) -> Box<dyn System> {
        Box::new(self.clone())
    }

    fn is_executable_file(&self, _: &CStr) -> bool {
        todo!()
    }
}
