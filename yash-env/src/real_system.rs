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

use super::ChildProcess;
use super::Env;
use super::System;
use crate::io::Fd;
use async_trait::async_trait;
use nix::errno::Errno;
use nix::libc::{S_IFMT, S_IFREG};
use nix::sys::stat::stat;
use nix::unistd::access;
use nix::unistd::AccessFlags;
use nix::unistd::Pid;
use std::convert::Infallible;
use std::ffi::CStr;
use std::ffi::CString;
use std::future::Future;
use std::pin::Pin;

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
#[derive(Debug)]
pub struct RealSystem;

impl System for RealSystem {
    fn is_executable_file(&self, path: &CStr) -> bool {
        is_regular_file(path) && is_executable(path)
    }

    fn dup(&mut self, from: Fd, to_min: Fd, cloexec: bool) -> nix::Result<Fd> {
        use nix::fcntl::FcntlArg::{F_DUPFD, F_DUPFD_CLOEXEC};
        let arg = if cloexec { F_DUPFD_CLOEXEC } else { F_DUPFD };
        nix::fcntl::fcntl(from.0, arg(to_min.0)).map(Fd)
    }

    fn dup2(&mut self, from: Fd, to: Fd) -> nix::Result<Fd> {
        loop {
            use nix::Error::Sys;
            match nix::unistd::dup2(from.0, to.0) {
                Ok(fd) => return Ok(Fd(fd)),
                Err(Sys(Errno::EINTR)) => (),
                Err(e) => return Err(e),
            }
        }
    }

    fn close(&mut self, fd: Fd) -> nix::Result<()> {
        loop {
            use nix::Error::Sys;
            match nix::unistd::close(fd.0) {
                Err(Sys(Errno::EBADF)) => return Ok(()),
                Err(Sys(Errno::EINTR)) => (),
                other => return other,
            }
        }
    }

    /// Creates a new child process.
    ///
    /// This implementation calls the `fork` system call and returns both in the
    /// parent and child process. In the parent, the `run` function of the
    /// returned `ChildProcess` ignores arguments and returns the child process
    /// ID. In the child, the `run` function runs the task and exits the
    /// process.
    unsafe fn new_child_process(&mut self) -> nix::Result<Box<dyn ChildProcess>> {
        use nix::unistd::ForkResult::*;
        match nix::unistd::fork()? {
            Parent { child } => Ok(Box::new(DummyChildProcess {
                child_process_id: child,
            })),
            Child => Ok(Box::new(RealChildProcess)),
        }
    }

    fn wait(&mut self) -> nix::Result<nix::sys::wait::WaitStatus> {
        use nix::sys::wait::WaitPidFlag;
        let options = WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED | WaitPidFlag::WNOHANG;
        nix::sys::wait::waitpid(None, options.into())
    }

    /// Reports updated status of a child process.
    ///
    /// This implementation blocks inside the function and returns a future that
    /// will immediately return a `Ready`.
    fn wait_sync(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = nix::Result<nix::sys::wait::WaitStatus>> + '_>> {
        use nix::sys::wait::WaitPidFlag;
        let options = WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED;
        // TODO Should set WNOHANG too
        let result = nix::sys::wait::waitpid(None, options.into());
        Box::pin(std::future::ready(result))
    }

    fn execve(
        &mut self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> nix::Result<Infallible> {
        loop {
            // TODO Use Result::into_err
            let result = nix::unistd::execve(path, args, envs);
            if result != Err(Errno::EINTR.into()) {
                return result;
            }
        }
    }
}

/// Implementor of [`ChildProcess`] that is returned from
/// [`RealSystem::new_child_process`] in the parent process.
#[derive(Debug)]
struct DummyChildProcess {
    child_process_id: Pid,
}

#[async_trait(?Send)]
impl ChildProcess for DummyChildProcess {
    async fn run(&mut self, _env: &mut Env, _task: super::ChildProcessTask) -> Pid {
        self.child_process_id
    }
}

/// Implementor of [`ChildProcess`] that is returned from
/// [`RealSystem::new_child_process`] in the child process.
#[derive(Debug)]
struct RealChildProcess;

#[async_trait(?Send)]
impl ChildProcess for RealChildProcess {
    async fn run(
        &mut self,
        env: &mut Env,
        mut task: Box<dyn for<'a> FnMut(&'a mut Env) -> Pin<Box<dyn Future<Output = ()> + 'a>>>,
    ) -> Pid {
        task(env).await;
        std::process::exit(env.exit_status.0)
    }
}
