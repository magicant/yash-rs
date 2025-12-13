// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! Unusable implementation of [`System`]

use super::resource::{LimitPair, Resource};
use super::{
    ChildProcessStarter, Dir, Disposition, FdFlag, FlexFuture, Mode, OfdAccess, OpenFlag, Result,
    SigmaskOp, Stat, System, Times,
};
use crate::io::Fd;
use crate::job::{Pid, ProcessState};
use crate::path::{Path, PathBuf};
use crate::semantics::ExitStatus;
use crate::signal;
use crate::str::UnixString;
use enumset::EnumSet;
use std::convert::Infallible;
use std::ffi::{CStr, CString, c_int};
use std::io::SeekFrom;
use std::time::{Duration, Instant};

/// Dummy system that is unusable
///
/// All methods of this system panic when called.
#[derive(Clone, Debug)]
pub struct UnusableSystem;

#[allow(unused_variables)]
impl System for UnusableSystem {
    fn fstat(&self, fd: Fd) -> Result<Stat> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn fstatat(&self, dir_fd: Fd, path: &CStr, follow_symlinks: bool) -> Result<Stat> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn is_executable_file(&self, path: &CStr) -> bool {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn is_directory(&self, path: &CStr) -> bool {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn pipe(&mut self) -> Result<(Fd, Fd)> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn dup(&mut self, from: Fd, to_min: Fd, flags: EnumSet<FdFlag>) -> Result<Fd> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn dup2(&mut self, from: Fd, to: Fd) -> Result<Fd> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn open(
        &mut self,
        path: &CStr,
        access: OfdAccess,
        flags: EnumSet<OpenFlag>,
        mode: Mode,
    ) -> Result<Fd> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn open_tmpfile(&mut self, parent_dir: &Path) -> Result<Fd> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn close(&mut self, fd: Fd) -> Result<()> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn ofd_access(&self, fd: Fd) -> Result<OfdAccess> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn get_and_set_nonblocking(&mut self, fd: Fd, nonblocking: bool) -> Result<bool> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn fcntl_getfd(&self, fd: Fd) -> Result<EnumSet<FdFlag>> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn fcntl_setfd(&mut self, fd: Fd, flags: EnumSet<FdFlag>) -> Result<()> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn isatty(&self, fd: Fd) -> bool {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> Result<usize> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn write(&mut self, fd: Fd, buffer: &[u8]) -> Result<usize> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn lseek(&mut self, fd: Fd, position: SeekFrom) -> Result<u64> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn fdopendir(&mut self, fd: Fd) -> Result<Box<dyn Dir>> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn opendir(&mut self, path: &CStr) -> Result<Box<dyn Dir>> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn umask(&mut self, new_mask: Mode) -> Mode {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn now(&self) -> Instant {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn times(&self) -> Result<Times> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn validate_signal(&self, number: signal::RawNumber) -> Option<(signal::Name, signal::Number)> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn sigmask(
        &mut self,
        op: Option<(SigmaskOp, &[signal::Number])>,
        old_mask: Option<&mut Vec<signal::Number>>,
    ) -> Result<()> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn get_sigaction(&self, signal: signal::Number) -> Result<Disposition> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn sigaction(&mut self, signal: signal::Number, action: Disposition) -> Result<Disposition> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn caught_signals(&mut self) -> Vec<signal::Number> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn kill(&mut self, target: Pid, signal: Option<signal::Number>) -> FlexFuture<Result<()>> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn raise(&mut self, signal: signal::Number) -> FlexFuture<Result<()>> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn select(
        &mut self,
        readers: &mut Vec<Fd>,
        writers: &mut Vec<Fd>,
        timeout: Option<Duration>,
        signal_mask: Option<&[signal::Number]>,
    ) -> Result<c_int> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn getsid(&self, pid: Pid) -> Result<Pid> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn getpid(&self) -> Pid {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn getppid(&self) -> Pid {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn getpgrp(&self) -> Pid {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn setpgid(&mut self, pid: Pid, pgid: Pid) -> Result<()> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn tcgetpgrp(&self, fd: Fd) -> Result<Pid> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn tcsetpgrp(&mut self, fd: Fd, pgid: Pid) -> FlexFuture<Result<()>> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn new_child_process(&mut self) -> Result<ChildProcessStarter> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn wait(&mut self, target: Pid) -> Result<Option<(Pid, ProcessState)>> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn execve(
        &mut self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> FlexFuture<Result<Infallible>> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn exit(&mut self, exit_status: ExitStatus) -> FlexFuture<Infallible> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn getcwd(&self) -> Result<PathBuf> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn chdir(&mut self, path: &CStr) -> Result<()> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn getuid(&self) -> super::Uid {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn geteuid(&self) -> super::Uid {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn getgid(&self) -> super::Gid {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn getegid(&self) -> super::Gid {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn getpwnam_dir(&self, name: &CStr) -> Result<Option<PathBuf>> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn confstr_path(&self) -> Result<UnixString> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn shell_path(&self) -> CString {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn getrlimit(&self, resource: Resource) -> Result<LimitPair> {
        unimplemented!("UnusableSystem provides no functionality")
    }

    fn setrlimit(&mut self, resource: Resource, limits: LimitPair) -> Result<()> {
        unimplemented!("UnusableSystem provides no functionality")
    }
}
