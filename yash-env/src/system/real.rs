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

use super::error::error_m1;
use super::AtFlags;
use super::ChildProcessStarter;
use super::Dir;
use super::DirEntry;
#[cfg(doc)]
use super::Env;
use super::Errno;
use super::FdFlag;
use super::FdSet;
use super::FileStat;
use super::Mode;
use super::OFlag;
use super::SigSet;
use super::SigmaskHow;
use super::Signal;
use super::System;
use super::TimeSpec;
use crate::io::Fd;
use crate::job::Pid;
use crate::SignalHandling;
use nix::libc::DIR;
use nix::sys::signal::SaFlags;
use nix::sys::signal::SigAction;
use nix::sys::signal::SigHandler;
use std::convert::Infallible;
use std::convert::TryInto;
use std::ffi::c_int;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsStr;
use std::io::Error;
use std::io::SeekFrom;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::IntoRawFd;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::atomic::compiler_fence;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering;
use std::time::Instant;

fn is_executable(path: &CStr) -> bool {
    unsafe { libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::X_OK, libc::AT_EACCESS) == 0 }
}

fn is_regular_file(path: &CStr) -> bool {
    // TODO Use RealSystem::fstatat
    let mut stat = MaybeUninit::uninit();
    unsafe {
        libc::stat(path.as_ptr(), stat.as_mut_ptr()) == 0 && {
            let stat = stat.assume_init();
            stat.st_mode & libc::S_IFMT == libc::S_IFREG
        }
    }
}

static CAUGHT_SIGNALS: [AtomicIsize; 8] = {
    // In the array creation, the repeat operand must be const.
    #[allow(clippy::declare_interior_mutable_const)]
    const SIGNAL_SLOT: AtomicIsize = AtomicIsize::new(0);
    [SIGNAL_SLOT; 8]
};

/// Signal catching function.
///
/// TODO Elaborate
extern "C" fn catch_signal(signal: c_int) {
    // This function can only perform async-signal-safe operations.
    // Performing unsafe operations is undefined behavior!

    // Find an unused slot (having a value of 0) in CAUGHT_SIGNALS and write the
    // signal number into it.
    // If there is a slot having a value of the signal already, do nothing.
    // If there is no available slot, the signal will be lost!
    let signal = signal as isize;
    for slot in &CAUGHT_SIGNALS {
        match slot.compare_exchange(0, signal, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(slot_value) if slot_value == signal => break,
            _ => continue,
        }
    }
}

/// Implementation of `System` that actually interacts with the system.
///
/// `RealSystem` is an empty `struct` because the underlying operating system
/// manages the system's internal state.
#[derive(Debug)]
pub struct RealSystem(());

impl RealSystem {
    /// Returns an instance of `RealSystem`.
    ///
    /// # Safety
    ///
    /// This function is marked `unsafe` because improper use of `RealSystem`
    /// may lead to undefined behavior. Remember that most operations performed
    /// on the system by [`Env`] are not thread-safe. You should never use
    /// `RealSystem` in a multi-threaded program, and it is your responsibility
    /// to make sure you are using only one instance of `ReadSystem` in the
    /// process.
    pub unsafe fn new() -> Self {
        RealSystem(())
    }
}

impl System for RealSystem {
    fn fstat(&self, fd: Fd) -> Result<FileStat, Error> {
        let mut stat = MaybeUninit::uninit();
        unsafe {
            let result = libc::fstat(fd.0, stat.as_mut_ptr());
            error_m1(result)?;
            Ok(stat.assume_init().into())
        }
    }

    fn fstatat(&self, dir_fd: Fd, path: &CStr, flags: AtFlags) -> Result<FileStat, Error> {
        let mut stat = MaybeUninit::uninit();
        unsafe {
            let result = libc::fstatat(dir_fd.0, path.as_ptr(), stat.as_mut_ptr(), flags.bits());
            error_m1(result)?;
            Ok(stat.assume_init().into())
        }
    }

    fn is_executable_file(&self, path: &CStr) -> bool {
        is_regular_file(path) && is_executable(path)
    }

    fn pipe(&mut self) -> nix::Result<(Fd, Fd)> {
        nix::unistd::pipe().map(|(reader, writer)| (Fd(reader), Fd(writer)))
    }

    fn dup(&mut self, from: Fd, to_min: Fd, flags: FdFlag) -> nix::Result<Fd> {
        let arg = if flags.contains(FdFlag::FD_CLOEXEC) {
            nix::fcntl::FcntlArg::F_DUPFD_CLOEXEC
        } else {
            nix::fcntl::FcntlArg::F_DUPFD
        };
        nix::fcntl::fcntl(from.0, arg(to_min.0)).map(Fd)
    }

    fn dup2(&mut self, from: Fd, to: Fd) -> nix::Result<Fd> {
        loop {
            match nix::unistd::dup2(from.0, to.0) {
                Ok(fd) => return Ok(Fd(fd)),
                Err(Errno::EINTR) => (),
                Err(e) => return Err(e),
            }
        }
    }

    fn open(&mut self, path: &CStr, option: OFlag, mode: Mode) -> nix::Result<Fd> {
        nix::fcntl::open(path, option, mode).map(Fd)
    }

    fn open_tmpfile(&mut self, parent_dir: &Path) -> nix::Result<Fd> {
        match tempfile::tempfile_in(parent_dir) {
            Ok(file) => Ok(Fd(file.into_raw_fd())),
            Err(error) => {
                let errno = error.raw_os_error().unwrap_or(0);
                Err(Errno::from_i32(errno))
            }
        }
    }

    fn close(&mut self, fd: Fd) -> Result<(), Error> {
        loop {
            let result = unsafe { libc::close(fd.0) };
            let result = error_m1(result);
            match &result {
                Ok(()) => return Ok(()),
                Err(error) => match error.raw_os_error() {
                    Some(libc::EBADF) => return Ok(()),
                    // TODO: Maybe it is a bad idea to call `close` again on `EINTR`
                    Some(libc::EINTR) => continue,
                    _ => return result,
                },
            }
        }
    }

    fn fcntl_getfl(&self, fd: Fd) -> nix::Result<OFlag> {
        nix::fcntl::fcntl(fd.0, nix::fcntl::FcntlArg::F_GETFL).map(OFlag::from_bits_truncate)
    }

    fn fcntl_setfl(&mut self, fd: Fd, flags: OFlag) -> nix::Result<()> {
        nix::fcntl::fcntl(fd.0, nix::fcntl::FcntlArg::F_SETFL(flags)).map(drop)
    }

    fn fcntl_getfd(&self, fd: Fd) -> nix::Result<FdFlag> {
        nix::fcntl::fcntl(fd.0, nix::fcntl::FcntlArg::F_GETFD).map(FdFlag::from_bits_truncate)
    }

    fn fcntl_setfd(&mut self, fd: Fd, flags: FdFlag) -> nix::Result<()> {
        nix::fcntl::fcntl(fd.0, nix::fcntl::FcntlArg::F_SETFD(flags)).map(drop)
    }

    fn isatty(&self, fd: Fd) -> nix::Result<bool> {
        nix::unistd::isatty(fd.0)
    }

    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> nix::Result<usize> {
        loop {
            let result = nix::unistd::read(fd.0, buffer);
            if result != Err(Errno::EINTR) {
                return result;
            }
        }
    }

    fn write(&mut self, fd: Fd, buffer: &[u8]) -> nix::Result<usize> {
        loop {
            let result = nix::unistd::write(fd.0, buffer);
            if result != Err(Errno::EINTR) {
                return result;
            }
        }
    }

    fn lseek(&mut self, fd: Fd, position: SeekFrom) -> nix::Result<u64> {
        use nix::unistd::Whence::*;
        let (offset, whence) = match position {
            SeekFrom::Start(offset) => {
                let offset = offset.try_into().map_err(|_| Errno::EOVERFLOW)?;
                (offset, SeekSet)
            }
            SeekFrom::End(offset) => (offset, SeekEnd),
            SeekFrom::Current(offset) => (offset, SeekCur),
        };
        nix::unistd::lseek(fd.0, offset, whence).map(|new_offset| new_offset as u64)
    }

    fn fdopendir(&mut self, fd: Fd) -> nix::Result<Box<dyn Dir>> {
        let dir = unsafe { nix::libc::fdopendir(fd.0) };
        let dir = NonNull::new(dir).ok_or_else(Errno::last)?;
        Ok(Box::new(RealDir(dir)))
    }

    fn opendir(&mut self, path: &CStr) -> nix::Result<Box<dyn Dir>> {
        let dir = unsafe { nix::libc::opendir(path.as_ptr()) };
        let dir = NonNull::new(dir).ok_or_else(Errno::last)?;
        Ok(Box::new(RealDir(dir)))
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sigmask(
        &mut self,
        how: SigmaskHow,
        set: Option<&SigSet>,
        oldset: Option<&mut SigSet>,
    ) -> nix::Result<()> {
        nix::sys::signal::sigprocmask(how, set, oldset)
    }

    fn sigaction(
        &mut self,
        signal: Signal,
        handling: SignalHandling,
    ) -> nix::Result<SignalHandling> {
        let handler = match handling {
            SignalHandling::Default => SigHandler::SigDfl,
            SignalHandling::Ignore => SigHandler::SigIgn,
            SignalHandling::Catch => SigHandler::Handler(catch_signal),
        };
        let new_action = SigAction::new(handler, SaFlags::empty(), SigSet::empty());
        // SAFETY: The `catch_signal` function only accesses atomic variables.
        let old_action = unsafe { nix::sys::signal::sigaction(signal, &new_action) }?;
        let old_handling = match old_action.handler() {
            SigHandler::SigDfl => SignalHandling::Default,
            SigHandler::SigIgn => SignalHandling::Ignore,
            SigHandler::Handler(_) | SigHandler::SigAction(_) => SignalHandling::Catch,
        };
        Ok(old_handling)
    }

    fn caught_signals(&mut self) -> Vec<Signal> {
        let mut signals = Vec::new();
        for slot in &CAUGHT_SIGNALS {
            // Need a fence to ensure we examine the slots in order.
            compiler_fence(Ordering::Acquire);

            let signal = slot.swap(0, Ordering::Relaxed);
            if signal == 0 {
                // The `catch_signal` function always fills the first unused
                // slot, so there is no more slot filled with a signal.
                break;
            }

            let signal = signal as c_int;
            if let Ok(signal) = signal.try_into() {
                signals.push(signal)
            } else {
                // ignore unknown signal
            }
        }
        signals
    }

    fn kill(&mut self, target: Pid, signal: Option<Signal>) -> nix::Result<()> {
        nix::sys::signal::kill(target, signal)
    }

    fn select(
        &mut self,
        readers: &mut FdSet,
        writers: &mut FdSet,
        timeout: Option<&TimeSpec>,
        signal_mask: Option<&SigSet>,
    ) -> nix::Result<c_int> {
        nix::sys::select::pselect(None, readers, writers, None, timeout, signal_mask)
    }

    fn getpid(&self) -> Pid {
        nix::unistd::getpid()
    }

    fn getppid(&self) -> Pid {
        nix::unistd::getppid()
    }

    fn getpgrp(&self) -> Pid {
        nix::unistd::getpgrp()
    }

    fn setpgid(&mut self, pid: Pid, pgid: Pid) -> nix::Result<()> {
        nix::unistd::setpgid(pid, pgid)
    }

    fn tcsetpgrp(&mut self, fd: Fd, pgid: Pid) -> nix::Result<()> {
        nix::unistd::tcsetpgrp(fd.0, pgid)
    }

    /// Creates a new child process.
    ///
    /// This implementation calls the `fork` system call and returns both in the
    /// parent and child process. In the parent, the `run` function of the
    /// returned `ChildProcess` ignores arguments and returns the child process
    /// ID. In the child, the `run` function runs the task and exits the
    /// process.
    fn new_child_process(&mut self) -> nix::Result<ChildProcessStarter> {
        use nix::unistd::ForkResult::*;
        // SAFETY: As stated on RealSystem::new, the caller is responsible for
        // making only one instance of RealSystem in the process.
        match unsafe { nix::unistd::fork()? } {
            Parent { child } => Ok(Box::new(move |_env, _task| Box::pin(async move { child }))),
            Child => Ok(Box::new(|env, task| {
                Box::pin(async move {
                    task(env).await;
                    std::process::exit(env.exit_status.0)
                })
            })),
        }
    }

    fn wait(&mut self, target: Pid) -> nix::Result<super::WaitStatus> {
        use nix::sys::wait::WaitPidFlag;
        let options = WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED | WaitPidFlag::WNOHANG;
        nix::sys::wait::waitpid(target, options.into())
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
            if result != Err(Errno::EINTR) {
                return result;
            }
        }
    }

    fn getcwd(&self) -> nix::Result<std::path::PathBuf> {
        nix::unistd::getcwd()
    }

    fn getpwnam_dir(&self, name: &str) -> nix::Result<Option<std::path::PathBuf>> {
        nix::unistd::User::from_name(name).map(|o| o.map(|passwd| passwd.dir))
    }
}

/// Implementor of [`Dir`] that iterates on a real directory
#[derive(Debug)]
struct RealDir(NonNull<DIR>);

impl Drop for RealDir {
    fn drop(&mut self) {
        unsafe {
            nix::libc::closedir(self.0.as_ptr());
        }
    }
}

impl Dir for RealDir {
    fn next(&mut self) -> nix::Result<Option<DirEntry>> {
        Errno::clear();
        let entry = unsafe { nix::libc::readdir(self.0.as_ptr()) };
        let errno = Errno::last();
        match NonNull::new(entry) {
            None if errno != Errno::UnknownErrno => Err(errno),
            None => Ok(None),
            Some(mut entry) => unsafe {
                let entry = entry.as_mut();
                let name = CStr::from_ptr(entry.d_name.as_ptr());
                let name = OsStr::from_bytes(name.to_bytes());
                Ok(Some(DirEntry { name }))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_system_directory_entries() {
        let mut system = unsafe { RealSystem::new() };
        let path = CString::new(".").unwrap();
        let mut dir = system.opendir(&path).unwrap();
        let mut count = 0;
        while dir.next().unwrap().is_some() {
            count += 1;
        }
        assert!(count > 0);
    }

    // This test depends on static variables.
    #[test]
    fn real_system_caught_signals() {
        unsafe {
            let mut system = RealSystem::new();
            let result = system.caught_signals();
            assert_eq!(result, []);

            catch_signal(Signal::SIGINT as c_int);
            catch_signal(Signal::SIGTERM as c_int);
            catch_signal(Signal::SIGTERM as c_int);
            catch_signal(Signal::SIGCHLD as c_int);

            let result = system.caught_signals();
            assert_eq!(result, [Signal::SIGINT, Signal::SIGTERM, Signal::SIGCHLD]);
            let result = system.caught_signals();
            assert_eq!(result, []);
        }
    }
}
