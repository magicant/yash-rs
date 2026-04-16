// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
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

//! Trait implementations for `Concurrent<S>` that delegate to the inner system `S`

use super::super::resource::{LimitPair, Resource};
use super::super::{
    Chdir, ChildProcessStarter, Clock, Close, CpuTimes, Dir, Dup, Exec, Exit, Fcntl, FdFlag, Fork,
    Fstat, GetCwd, GetPid, GetPw, GetRlimit, GetSigaction, GetUid, Gid, IsExecutableFile, Isatty,
    Mode, OfdAccess, Open, OpenFlag, Pipe, Result, Seek, SendSignal, SetPgid, SetRlimit, ShellPath,
    Sigaction, Sigmask, SigmaskOp, Signals, Sysconf, TcGetPgrp, TcSetPgrp, Times, Uid, Umask, Wait,
    signal,
};
use super::Concurrent;
use crate::io::Fd;
use crate::job::{Pid, ProcessState};
use crate::path::PathBuf;
use crate::semantics::ExitStatus;
use enumset::EnumSet;
use std::convert::Infallible;
use std::ffi::{CStr, CString};
use std::io::SeekFrom;
use std::ops::RangeInclusive;
use std::time::Instant;
use unix_str::UnixString;

impl<S> Fstat for Concurrent<S>
where
    S: Fstat,
{
    type Stat = S::Stat;

    #[inline]
    fn fstat(&self, fd: Fd) -> Result<Self::Stat> {
        self.inner.fstat(fd)
    }
    #[inline]
    fn fstatat(&self, dir_fd: Fd, path: &CStr, follow_symlinks: bool) -> Result<Self::Stat> {
        self.inner.fstatat(dir_fd, path, follow_symlinks)
    }
    #[inline]
    fn is_directory(&self, path: &CStr) -> bool {
        self.inner.is_directory(path)
    }
    #[inline]
    fn fd_is_pipe(&self, fd: Fd) -> bool {
        self.inner.fd_is_pipe(fd)
    }
}

impl<S> IsExecutableFile for Concurrent<S>
where
    S: IsExecutableFile,
{
    #[inline]
    fn is_executable_file(&self, path: &CStr) -> bool {
        self.inner.is_executable_file(path)
    }
}

impl<S> Pipe for Concurrent<S>
where
    S: Pipe,
{
    #[inline]
    fn pipe(&self) -> Result<(Fd, Fd)> {
        self.inner.pipe()
    }
}

impl<S> Dup for Concurrent<S>
where
    S: Dup,
{
    #[inline]
    fn dup(&self, from: Fd, to_min: Fd, flags: EnumSet<FdFlag>) -> Result<Fd> {
        self.inner.dup(from, to_min, flags)
    }

    #[inline]
    fn dup2(&self, from: Fd, to: Fd) -> Result<Fd> {
        self.inner.dup2(from, to)
    }
}

/// This implementation does not (yet) support non-blocking open operations.
impl<S> Open for Concurrent<S>
where
    S: Open,
{
    #[inline]
    fn open(
        &self,
        path: &CStr,
        access: OfdAccess,
        flags: EnumSet<OpenFlag>,
        mode: Mode,
    ) -> impl Future<Output = Result<Fd>> + use<S> {
        self.inner.open(path, access, flags, mode)
    }

    #[inline]
    fn open_tmpfile(&self, parent_dir: &unix_path::Path) -> Result<Fd> {
        self.inner.open_tmpfile(parent_dir)
    }

    #[inline]
    fn fdopendir(&self, fd: Fd) -> Result<impl Dir + use<S>> {
        self.inner.fdopendir(fd)
    }

    #[inline]
    fn opendir(&self, path: &CStr) -> Result<impl Dir + use<S>> {
        self.inner.opendir(path)
    }
}

impl<S> Close for Concurrent<S>
where
    S: Close,
{
    #[inline]
    fn close(&self, fd: Fd) -> Result<()> {
        self.inner.close(fd)
    }
}

impl<S> Fcntl for Concurrent<S>
where
    S: Fcntl,
{
    #[inline]
    fn ofd_access(&self, fd: Fd) -> Result<OfdAccess> {
        self.inner.ofd_access(fd)
    }

    #[inline]
    fn get_and_set_nonblocking(&self, fd: Fd, nonblocking: bool) -> Result<bool> {
        self.inner.get_and_set_nonblocking(fd, nonblocking)
    }

    #[inline]
    fn fcntl_getfd(&self, fd: Fd) -> Result<EnumSet<FdFlag>> {
        self.inner.fcntl_getfd(fd)
    }

    #[inline]
    fn fcntl_setfd(&self, fd: Fd, flags: EnumSet<FdFlag>) -> Result<()> {
        self.inner.fcntl_setfd(fd, flags)
    }
}

impl<S> Seek for Concurrent<S>
where
    S: Seek,
{
    #[inline]
    fn lseek(&self, fd: Fd, position: SeekFrom) -> Result<u64> {
        self.inner.lseek(fd, position)
    }
}

impl<S> Umask for Concurrent<S>
where
    S: Umask,
{
    #[inline]
    fn umask(&self, new_mask: Mode) -> Mode {
        self.inner.umask(new_mask)
    }
}

impl<S> GetCwd for Concurrent<S>
where
    S: GetCwd,
{
    #[inline]
    fn getcwd(&self) -> Result<PathBuf> {
        self.inner.getcwd()
    }
}

impl<S> Chdir for Concurrent<S>
where
    S: Chdir,
{
    #[inline]
    fn chdir(&self, path: &CStr) -> Result<()> {
        self.inner.chdir(path)
    }
}

impl<S> Clock for Concurrent<S>
where
    S: Clock,
{
    #[inline]
    fn now(&self) -> Instant {
        self.inner.now()
    }
}

impl<S> Times for Concurrent<S>
where
    S: Times,
{
    #[inline]
    fn times(&self) -> Result<CpuTimes> {
        self.inner.times()
    }
}

impl<S> GetPid for Concurrent<S>
where
    S: GetPid,
{
    #[inline]
    fn getpid(&self) -> Pid {
        self.inner.getpid()
    }
    #[inline]
    fn getppid(&self) -> Pid {
        self.inner.getppid()
    }
    #[inline]
    fn getpgrp(&self) -> Pid {
        self.inner.getpgrp()
    }
    #[inline]
    fn getsid(&self, pid: Pid) -> Result<Pid> {
        self.inner.getsid(pid)
    }
}

impl<S> SetPgid for Concurrent<S>
where
    S: SetPgid,
{
    #[inline]
    fn setpgid(&self, pid: Pid, pgid: Pid) -> Result<()> {
        self.inner.setpgid(pid, pgid)
    }
}

impl<S> Signals for Concurrent<S>
where
    S: Signals,
{
    const SIGABRT: signal::Number = S::SIGABRT;
    const SIGALRM: signal::Number = S::SIGALRM;
    const SIGBUS: signal::Number = S::SIGBUS;
    const SIGCHLD: signal::Number = S::SIGCHLD;
    const SIGCLD: Option<signal::Number> = S::SIGCLD;
    const SIGCONT: signal::Number = S::SIGCONT;
    const SIGEMT: Option<signal::Number> = S::SIGEMT;
    const SIGFPE: signal::Number = S::SIGFPE;
    const SIGHUP: signal::Number = S::SIGHUP;
    const SIGILL: signal::Number = S::SIGILL;
    const SIGINFO: Option<signal::Number> = S::SIGINFO;
    const SIGINT: signal::Number = S::SIGINT;
    const SIGIO: Option<signal::Number> = S::SIGIO;
    const SIGIOT: signal::Number = S::SIGIOT;
    const SIGKILL: signal::Number = S::SIGKILL;
    const SIGLOST: Option<signal::Number> = S::SIGLOST;
    const SIGPIPE: signal::Number = S::SIGPIPE;
    const SIGPOLL: Option<signal::Number> = S::SIGPOLL;
    const SIGPROF: signal::Number = S::SIGPROF;
    const SIGPWR: Option<signal::Number> = S::SIGPWR;
    const SIGQUIT: signal::Number = S::SIGQUIT;
    const SIGSEGV: signal::Number = S::SIGSEGV;
    const SIGSTKFLT: Option<signal::Number> = S::SIGSTKFLT;
    const SIGSTOP: signal::Number = S::SIGSTOP;
    const SIGSYS: signal::Number = S::SIGSYS;
    const SIGTERM: signal::Number = S::SIGTERM;
    const SIGTHR: Option<signal::Number> = S::SIGTHR;
    const SIGTRAP: signal::Number = S::SIGTRAP;
    const SIGTSTP: signal::Number = S::SIGTSTP;
    const SIGTTIN: signal::Number = S::SIGTTIN;
    const SIGTTOU: signal::Number = S::SIGTTOU;
    const SIGURG: signal::Number = S::SIGURG;
    const SIGUSR1: signal::Number = S::SIGUSR1;
    const SIGUSR2: signal::Number = S::SIGUSR2;
    const SIGVTALRM: signal::Number = S::SIGVTALRM;
    const SIGWINCH: signal::Number = S::SIGWINCH;
    const SIGXCPU: signal::Number = S::SIGXCPU;
    const SIGXFSZ: signal::Number = S::SIGXFSZ;

    #[inline]
    fn sigrt_range(&self) -> Option<RangeInclusive<signal::Number>> {
        self.inner.sigrt_range()
    }

    const NAMED_SIGNALS: &'static [(&'static str, Option<signal::Number>)] = S::NAMED_SIGNALS;

    #[inline]
    fn iter_sigrt(&self) -> impl DoubleEndedIterator<Item = signal::Number> + use<S> {
        self.inner.iter_sigrt()
    }
    #[inline]
    fn to_signal_number<N: Into<signal::RawNumber>>(&self, number: N) -> Option<signal::Number> {
        self.inner.to_signal_number(number)
    }
    #[inline]
    fn sig2str<N: Into<signal::RawNumber>>(
        &self,
        signal: N,
    ) -> Option<std::borrow::Cow<'static, str>> {
        self.inner.sig2str(signal)
    }
    #[inline]
    fn str2sig(&self, name: &str) -> Option<signal::Number> {
        self.inner.str2sig(name)
    }
    #[inline]
    fn validate_signal(&self, number: signal::RawNumber) -> Option<(signal::Name, signal::Number)> {
        self.inner.validate_signal(number)
    }
    #[inline]
    fn signal_name_from_number(&self, number: signal::Number) -> signal::Name {
        self.inner.signal_name_from_number(number)
    }
    #[inline]
    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number> {
        self.inner.signal_number_from_name(name)
    }
}

/// Exposes the inner system's `sigmask` method.
///
/// This implementation of `Sigmask` simply delegates to the inner system's
/// `sigmask` method, which bypasses the internal state of `Concurrent` and may
/// prevent the [`peek`](Concurrent::peek) and [`select`](Concurrent::select)
/// methods from responding to received signals without race conditions. To
/// ensure that the signal mask is configured in a way that allows `Concurrent`
/// to respond to signals correctly, direct calls to `sigmask` should be
/// avoided, and, if necessary, only used to temporarily change the signal mask
/// for specific operations while ensuring that the original mask is restored
/// afterward before a next call to `peek`, `select`, or `set_disposition`.
impl<S> Sigmask for Concurrent<S>
where
    S: Sigmask,
{
    #[inline]
    fn sigmask(
        &self,
        op_and_signals: Option<(SigmaskOp, &[signal::Number])>,
        old_mask: Option<&mut Vec<signal::Number>>,
    ) -> impl Future<Output = Result<()>> + use<S> {
        self.inner.sigmask(op_and_signals, old_mask)
    }
}

impl<S> GetSigaction for Concurrent<S>
where
    S: GetSigaction,
{
    #[inline]
    fn get_sigaction(&self, signal: signal::Number) -> Result<signal::Disposition> {
        self.inner.get_sigaction(signal)
    }
}

/// Exposes the inner system's `sigaction` method.
///
/// This implementation of `Sigaction` simply delegates to the inner system's
/// `sigaction` method, which bypasses the internal state of `Concurrent` and
/// may prevent the [`peek`](Concurrent::peek) and
/// [`select`](Concurrent::select) methods from responding to received signals
/// without race conditions. To ensure that signal dispositions are configured
/// in a way that allows `Concurrent` to respond to signals correctly, direct
/// calls to `sigaction` should be avoided, and, if necessary, only used to
/// temporarily change the signal disposition for specific operations while
/// ensuring that the original disposition is restored afterward before a next
/// call to `peek`, `select`, or `set_disposition`.
///
/// The standard way to set a signal disposition to `Concurrent` is to use the
/// `set_disposition` method provided by the [`SignalSystem`] trait, which
/// ensures that the signal disposition and the signal mask are updated
/// consistently.
///
/// [`SignalSystem`]: crate::trap::SignalSystem
impl<S> Sigaction for Concurrent<S>
where
    S: Sigaction,
{
    #[inline]
    fn sigaction(
        &self,
        signal: signal::Number,
        disposition: signal::Disposition,
    ) -> Result<signal::Disposition> {
        self.inner.sigaction(signal, disposition)
    }
}

// CaughtSignals is not implemented for Concurrent<S> because Concurrent needs to
// control the signal dispositions and the signal mask itself to ensure that the
// select method can respond to received signals without race conditions. Instead,
// Concurrent<S> implements the SignalSystem trait, which provides the necessary
// methods for configuring signal dispositions and masks in a controlled manner.
//
// Note: Sigmask, GetSigaction, and Sigaction are implemented above as delegating
// implementations. However, direct calls to sigmask() and sigaction() bypass
// Concurrent's internal state and may prevent peek() and select() from responding
// to received signals without race conditions. To ensure correct behavior, use the
// set_disposition method from the SignalSystem trait instead. If direct sigmask or
// sigaction calls are necessary, temporarily change the mask/disposition and restore
// it afterward before calling peek(), select(), or set_disposition().

impl<S> SendSignal for Concurrent<S>
where
    S: SendSignal,
{
    #[inline]
    fn kill(
        &self,
        pid: Pid,
        signal: Option<signal::Number>,
    ) -> impl Future<Output = Result<()>> + use<S> {
        self.inner.kill(pid, signal)
    }
    #[inline]
    fn raise(&self, signal: signal::Number) -> impl Future<Output = Result<()>> + use<S> {
        self.inner.raise(signal)
    }
}

impl<S> Isatty for Concurrent<S>
where
    S: Isatty,
{
    #[inline]
    fn isatty(&self, fd: Fd) -> bool {
        self.inner.isatty(fd)
    }
}

impl<S> TcGetPgrp for Concurrent<S>
where
    S: TcGetPgrp,
{
    #[inline]
    fn tcgetpgrp(&self, fd: Fd) -> Result<Pid> {
        self.inner.tcgetpgrp(fd)
    }
}

impl<S> TcSetPgrp for Concurrent<S>
where
    S: TcSetPgrp,
{
    #[inline]
    fn tcsetpgrp(&self, fd: Fd, pgid: Pid) -> impl Future<Output = Result<()>> + use<S> {
        self.inner.tcsetpgrp(fd, pgid)
    }
}

// Concurrent<S> cannot implement Fork because the return type of the fork
// method does not match with that of the inner system S. Instead, Concurrent<S>
// provides an inherent method `new_child_process` that returns a
// `ChildProcessStarter<S>` as returned by the inner method.
// impl<S> Fork for Concurrent<S>
// where
//     S: Fork,
// {
//     #[inline]
//     fn new_child_process(&self) -> Result<ChildProcessStarter<Self>>
//     where
//         Self: Sized,
//     {
//         todo!()
//     }
// }

impl<S> Concurrent<S>
where
    S: Fork,
{
    /// Creates a new child process.
    ///
    /// Returns the `ChildProcessStarter<S>` returned by the inner system's
    /// [`Fork::new_child_process`] method. This method is an inherent method of
    /// `Concurrent<S>` instead of an implementation of the `Fork` trait because
    /// the return type does not match with that of the inner system `S`.
    #[inline]
    pub fn new_child_process(&self) -> Result<ChildProcessStarter<S>> {
        self.inner.new_child_process()
    }
}

impl<S> Wait for Concurrent<S>
where
    S: Wait,
{
    #[inline]
    fn wait(&self, target: Pid) -> Result<Option<(Pid, ProcessState)>> {
        self.inner.wait(target)
    }
}

impl<S> Exec for Concurrent<S>
where
    S: Exec,
{
    #[inline]
    fn execve(
        &self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> impl Future<Output = Result<Infallible>> + use<S> {
        self.inner.execve(path, args, envs)
    }
}

impl<S> Exit for Concurrent<S>
where
    S: Exit,
{
    #[inline]
    fn exit(&self, exit_status: ExitStatus) -> impl Future<Output = Infallible> + use<S> {
        self.inner.exit(exit_status)
    }
}

impl<S> GetUid for Concurrent<S>
where
    S: GetUid,
{
    #[inline]
    fn getuid(&self) -> Uid {
        self.inner.getuid()
    }
    #[inline]
    fn geteuid(&self) -> Uid {
        self.inner.geteuid()
    }
    #[inline]
    fn getgid(&self) -> Gid {
        self.inner.getgid()
    }
    #[inline]
    fn getegid(&self) -> Gid {
        self.inner.getegid()
    }
}

impl<S> GetPw for Concurrent<S>
where
    S: GetPw,
{
    #[inline]
    fn getpwnam_dir(&self, name: &CStr) -> Result<Option<PathBuf>> {
        self.inner.getpwnam_dir(name)
    }
}

impl<S> Sysconf for Concurrent<S>
where
    S: Sysconf,
{
    #[inline]
    fn confstr_path(&self) -> Result<UnixString> {
        self.inner.confstr_path()
    }
}

impl<S> ShellPath for Concurrent<S>
where
    S: ShellPath,
{
    #[inline]
    fn shell_path(&self) -> CString {
        self.inner.shell_path()
    }
}

impl<S> GetRlimit for Concurrent<S>
where
    S: GetRlimit,
{
    #[inline]
    fn getrlimit(&self, resource: Resource) -> Result<LimitPair> {
        self.inner.getrlimit(resource)
    }
}

impl<S> SetRlimit for Concurrent<S>
where
    S: SetRlimit,
{
    #[inline]
    fn setrlimit(&self, resource: Resource, limits: LimitPair) -> Result<()> {
        self.inner.setrlimit(resource, limits)
    }
}
