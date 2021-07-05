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
use nix::libc::{S_IFMT, S_IFREG};
use nix::sys::stat::stat;
use nix::unistd::access;
use nix::unistd::AccessFlags;
use std::convert::Infallible;
use std::ffi::CStr;
use std::ffi::CString;

fn is_executable(path: &CStr) -> bool {
    let flags = AccessFlags::X_OK;
    access(path, flags).is_ok()
    // TODO Should use eaccess
}

fn is_regular_file(path: &CStr) -> bool {
    match stat(path) {
        Ok(stat) => stat.st_mode & S_IFMT == S_IFREG,
        Err(_) => false,
    }
}

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
        is_regular_file(path) && is_executable(path)
    }

    unsafe fn fork(&mut self) -> nix::Result<nix::unistd::ForkResult> {
        nix::unistd::fork()
    }

    fn wait(&mut self) -> nix::Result<nix::sys::wait::WaitStatus> {
        use nix::sys::wait::WaitPidFlag;
        let options = WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED;
        // TODO Should set WNOHANG too
        nix::sys::wait::waitpid(None, options.into())
    }

    fn execve(
        &mut self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> nix::Result<Infallible> {
        loop {
            use nix::errno::Errno::EINTR;
            // TODO Use Result::into_err
            let result = nix::unistd::execve(path, args, envs);
            if result != Err(nix::Error::Sys(EINTR)) {
                return result;
            }
        }
    }
}
