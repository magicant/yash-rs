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

use super::resource::LimitPair;
use super::resource::Resource;
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
use super::Result;
use super::SigSet;
use super::SigmaskHow;
use super::Signal;
use super::Signal2;
use super::System;
use super::TimeSpec;
use super::Times;
use super::UnknownSignalError;
use crate::io::Fd;
use crate::job::Pid;
use crate::job::ProcessState;
use crate::semantics::ExitStatus;
use crate::SignalHandling;
use nix::errno::Errno as NixErrno;
use nix::libc::DIR;
use nix::libc::{S_IFDIR, S_IFMT, S_IFREG};
use nix::sys::signal::SaFlags;
use nix::sys::signal::SigAction;
use nix::sys::signal::SigHandler;
use nix::sys::stat::stat;
use nix::sys::wait::WaitStatus;
use nix::unistd::AccessFlags;
use std::convert::Infallible;
use std::convert::TryInto;
use std::ffi::c_int;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::future::Future;
use std::io::SeekFrom;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt as _;
use std::os::unix::io::IntoRawFd;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::compiler_fence;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering;
use std::time::Instant;

trait ErrnoIfM1: PartialEq + Sized {
    const MINUS_1: Self;

    /// Convenience function to convert a result of -1 to an `Error` with the
    /// current `errno`.
    ///
    /// This function is intended to be used just after calling a function that
    /// returns -1 on error and sets `errno` to the error number. This function
    /// filters out the `-1` result and returns an error containing the current
    /// `errno`.
    fn errno_if_m1(self) -> Result<Self> {
        if self == Self::MINUS_1 {
            Err(Errno::last())
        } else {
            Ok(self)
        }
    }
}

impl ErrnoIfM1 for i8 {
    const MINUS_1: Self = -1;
}
impl ErrnoIfM1 for i16 {
    const MINUS_1: Self = -1;
}
impl ErrnoIfM1 for i32 {
    const MINUS_1: Self = -1;
}
impl ErrnoIfM1 for i64 {
    const MINUS_1: Self = -1;
}
impl ErrnoIfM1 for isize {
    const MINUS_1: Self = -1;
}

// TODO Should use AT_EACCESS on all platforms
#[cfg(not(target_os = "redox"))]
fn is_executable(path: &CStr) -> bool {
    nix::unistd::faccessat(None, path, AccessFlags::X_OK, AtFlags::AT_EACCESS).is_ok()
}
#[cfg(target_os = "redox")]
fn is_executable(path: &CStr) -> bool {
    nix::unistd::access(path, AccessFlags::X_OK).is_ok()
}

fn is_regular_file(path: &CStr) -> bool {
    matches!(stat(path), Ok(stat) if stat.st_mode & S_IFMT == S_IFREG)
}

fn is_directory(path: &CStr) -> bool {
    matches!(stat(path), Ok(stat) if stat.st_mode & S_IFMT == S_IFDIR)
}

/// Array of slots to store caught signals.
///
/// This array is used to store caught signals. All slots are initialized with
/// 0, which indicates that the slot is available. When a signal is caught, the
/// signal number is written into one of unoccupied slots.
static CAUGHT_SIGNALS: [AtomicIsize; 8] = {
    // In the array creation, the repeat operand must be const.
    #[allow(clippy::declare_interior_mutable_const)]
    const SIGNAL_SLOT: AtomicIsize = AtomicIsize::new(0);
    [SIGNAL_SLOT; 8]
};

/// Signal catching function.
///
/// This function is set as a signal handler for all signals that the shell
/// wants to catch. When a signal is caught, the signal number is written into
/// one of the slots in [`CAUGHT_SIGNALS`].
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
    fn fstat(&self, fd: Fd) -> Result<FileStat> {
        Ok(nix::sys::stat::fstat(fd.0)?)
    }

    fn fstatat(&self, dir_fd: Fd, path: &CStr, flags: AtFlags) -> Result<FileStat> {
        Ok(nix::sys::stat::fstatat(dir_fd.0, path, flags)?)
    }

    fn is_executable_file(&self, path: &CStr) -> bool {
        is_regular_file(path) && is_executable(path)
    }

    fn is_directory(&self, path: &CStr) -> bool {
        is_directory(path)
    }

    fn pipe(&mut self) -> Result<(Fd, Fd)> {
        let (reader, writer) = nix::unistd::pipe()?;
        Ok((Fd(reader), Fd(writer)))
    }

    fn dup(&mut self, from: Fd, to_min: Fd, flags: FdFlag) -> Result<Fd> {
        let arg = if flags.contains(FdFlag::FD_CLOEXEC) {
            nix::fcntl::FcntlArg::F_DUPFD_CLOEXEC
        } else {
            nix::fcntl::FcntlArg::F_DUPFD
        };
        Ok(Fd(nix::fcntl::fcntl(from.0, arg(to_min.0))?))
    }

    fn dup2(&mut self, from: Fd, to: Fd) -> Result<Fd> {
        loop {
            match nix::unistd::dup2(from.0, to.0) {
                Ok(fd) => return Ok(Fd(fd)),
                Err(NixErrno::EINTR) => (),
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn open(&mut self, path: &CStr, option: OFlag, mode: Mode) -> Result<Fd> {
        Ok(Fd(nix::fcntl::open(path, option, mode)?))
    }

    fn open_tmpfile(&mut self, parent_dir: &Path) -> Result<Fd> {
        let file = tempfile::tempfile_in(parent_dir)
            .map_err(|errno| Errno(errno.raw_os_error().unwrap_or(0)))?;
        let fd = Fd(file.into_raw_fd());

        // Clear the CLOEXEC flag
        _ = self.fcntl_setfd(fd, FdFlag::empty());

        Ok(fd)
    }

    fn close(&mut self, fd: Fd) -> Result<()> {
        loop {
            match nix::unistd::close(fd.0) {
                Err(NixErrno::EBADF) => return Ok(()),
                Err(NixErrno::EINTR) => (),
                other => return Ok(other?),
            }
        }
    }

    fn fcntl_getfl(&self, fd: Fd) -> Result<OFlag> {
        let bits = nix::fcntl::fcntl(fd.0, nix::fcntl::FcntlArg::F_GETFL)?;
        Ok(OFlag::from_bits_truncate(bits))
    }

    fn fcntl_setfl(&mut self, fd: Fd, flags: OFlag) -> Result<()> {
        let _ = nix::fcntl::fcntl(fd.0, nix::fcntl::FcntlArg::F_SETFL(flags))?;
        Ok(())
    }

    fn fcntl_getfd(&self, fd: Fd) -> Result<FdFlag> {
        let bits = nix::fcntl::fcntl(fd.0, nix::fcntl::FcntlArg::F_GETFD)?;
        Ok(FdFlag::from_bits_truncate(bits))
    }

    fn fcntl_setfd(&mut self, fd: Fd, flags: FdFlag) -> Result<()> {
        let _ = nix::fcntl::fcntl(fd.0, nix::fcntl::FcntlArg::F_SETFD(flags))?;
        Ok(())
    }

    fn isatty(&self, fd: Fd) -> Result<bool> {
        Ok(nix::unistd::isatty(fd.0)?)
    }

    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> Result<usize> {
        loop {
            let result = nix::unistd::read(fd.0, buffer);
            if result != Err(NixErrno::EINTR) {
                return Ok(result?);
            }
        }
    }

    fn write(&mut self, fd: Fd, buffer: &[u8]) -> Result<usize> {
        loop {
            let result = nix::unistd::write(fd.0, buffer);
            if result != Err(NixErrno::EINTR) {
                return Ok(result?);
            }
        }
    }

    fn lseek(&mut self, fd: Fd, position: SeekFrom) -> Result<u64> {
        use nix::unistd::Whence::*;
        let (offset, whence) = match position {
            SeekFrom::Start(offset) => {
                let offset = offset.try_into().map_err(|_| NixErrno::EOVERFLOW)?;
                (offset, SeekSet)
            }
            SeekFrom::End(offset) => (offset, SeekEnd),
            SeekFrom::Current(offset) => (offset, SeekCur),
        };
        let new_offset = nix::unistd::lseek(fd.0, offset, whence)?;
        Ok(new_offset as u64)
    }

    fn fdopendir(&mut self, fd: Fd) -> Result<Box<dyn Dir>> {
        let dir = unsafe { nix::libc::fdopendir(fd.0) };
        let dir = NonNull::new(dir).ok_or_else(NixErrno::last)?;
        Ok(Box::new(RealDir(dir)))
    }

    fn opendir(&mut self, path: &CStr) -> Result<Box<dyn Dir>> {
        let dir = unsafe { nix::libc::opendir(path.as_ptr()) };
        let dir = NonNull::new(dir).ok_or_else(NixErrno::last)?;
        Ok(Box::new(RealDir(dir)))
    }

    fn umask(&mut self, mask: Mode) -> Mode {
        nix::sys::stat::umask(mask)
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn times(&self) -> Result<Times> {
        let mut tms = MaybeUninit::<nix::libc::tms>::uninit();
        let raw_result = unsafe { nix::libc::times(tms.as_mut_ptr()) };
        if raw_result == (-1) as _ {
            return Err(Errno::last());
        }
        let tms = unsafe { tms.assume_init() };

        let ticks_per_second = unsafe { nix::libc::sysconf(nix::libc::_SC_CLK_TCK) };
        if ticks_per_second <= 0 {
            return Err(Errno::last());
        }

        Ok(Times {
            self_user: tms.tms_utime as f64 / ticks_per_second as f64,
            self_system: tms.tms_stime as f64 / ticks_per_second as f64,
            children_user: tms.tms_cutime as f64 / ticks_per_second as f64,
            children_system: tms.tms_cstime as f64 / ticks_per_second as f64,
        })
    }

    #[inline]
    fn signal_to_raw_number(
        &self,
        signal: Signal2,
    ) -> std::result::Result<c_int, UnknownSignalError> {
        signal.to_raw()
    }

    #[inline]
    fn raw_number_to_signal(
        &self,
        number: c_int,
    ) -> std::result::Result<Signal2, UnknownSignalError> {
        Signal2::try_from_raw(number)
    }

    fn sigmask(
        &mut self,
        how: SigmaskHow,
        set: Option<&[Signal]>,
        old_set: Option<&mut Vec<Signal>>,
    ) -> Result<()> {
        unsafe {
            let raw_set = match set {
                None => None,
                Some(set) => {
                    let mut raw_set = MaybeUninit::<nix::libc::sigset_t>::uninit();
                    nix::libc::sigemptyset(raw_set.as_mut_ptr()).errno_if_m1()?;
                    for &signal in set {
                        let number = signal as _; //self.signal_to_raw_number(signal)?;
                        nix::libc::sigaddset(raw_set.as_mut_ptr(), number).errno_if_m1()?;
                    }
                    Some(raw_set.assume_init())
                }
            };
            let mut old_set_pair = match old_set {
                None => None,
                Some(old_set) => {
                    let mut raw_old_set = MaybeUninit::<nix::libc::sigset_t>::uninit();
                    nix::libc::sigemptyset(raw_old_set.as_mut_ptr()).errno_if_m1()?;
                    Some((old_set, raw_old_set.assume_init()))
                }
            };

            let raw_set_ptr = raw_set
                .as_ref()
                .map_or(std::ptr::null(), |raw_set| raw_set as *const _);
            let raw_old_set_ptr = old_set_pair
                .as_mut()
                .map_or(std::ptr::null_mut(), |(_, raw_old_set)| {
                    raw_old_set as *mut _
                });

            let result = nix::libc::sigprocmask(how as _, raw_set_ptr, raw_old_set_ptr);
            result.errno_if_m1().map(drop)?;

            if let Some((old_set, raw_old_set)) = old_set_pair {
                old_set.clear();
                for signal in Signal::iterator() {
                    let number = signal as _; //self.signal_to_raw_number(signal)?;
                    if nix::libc::sigismember(&raw_old_set, number) == 1 {
                        old_set.push(signal);
                    }
                }
            }

            Ok(())
        }
    }

    fn sigaction(&mut self, signal: Signal, handling: SignalHandling) -> Result<SignalHandling> {
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

    fn kill(
        &mut self,
        target: Pid,
        signal: Option<Signal>,
    ) -> Pin<Box<(dyn Future<Output = Result<()>>)>> {
        Box::pin(async move {
            nix::sys::signal::kill(target.into(), signal)?;
            Ok(())
        })
    }

    fn select(
        &mut self,
        readers: &mut FdSet,
        writers: &mut FdSet,
        timeout: Option<&TimeSpec>,
        signal_mask: Option<&[Signal]>,
    ) -> Result<c_int> {
        use std::ptr::{null, null_mut};
        let nfds = readers.upper_bound().max(writers.upper_bound()).0;
        let readers = &mut readers.inner;
        let writers = &mut writers.inner;
        let errors = null_mut();
        let timeout = timeout.map_or(null(), |timeout| timeout.as_ref());
        let signal_mask = signal_mask.map(|mask| mask.iter().copied().collect::<SigSet>());
        let signal_mask = signal_mask.map_or(null(), |mask| mask.as_ref());
        let raw_result =
            unsafe { nix::libc::pselect(nfds, readers, writers, errors, timeout, signal_mask) };
        raw_result.errno_if_m1()
    }

    fn getpid(&self) -> Pid {
        nix::unistd::getpid().into()
    }

    fn getppid(&self) -> Pid {
        nix::unistd::getppid().into()
    }

    fn getpgrp(&self) -> Pid {
        nix::unistd::getpgrp().into()
    }

    fn setpgid(&mut self, pid: Pid, pgid: Pid) -> Result<()> {
        nix::unistd::setpgid(pid.into(), pgid.into())?;
        Ok(())
    }

    fn tcgetpgrp(&self, fd: Fd) -> Result<Pid> {
        let pgrp = nix::unistd::tcgetpgrp(fd.0)?;
        Ok(pgrp.into())
    }

    fn tcsetpgrp(&mut self, fd: Fd, pgid: Pid) -> Result<()> {
        nix::unistd::tcsetpgrp(fd.0, pgid.into())?;
        Ok(())
    }

    /// Creates a new child process.
    ///
    /// This implementation calls the `fork` system call and returns both in the
    /// parent and child process. In the parent, the returned
    /// `ChildProcessStarter` ignores any arguments and returns the child
    /// process ID. In the child, the starter runs the task and exits the
    /// process.
    fn new_child_process(&mut self) -> Result<ChildProcessStarter> {
        use nix::unistd::ForkResult::*;
        // SAFETY: As stated on RealSystem::new, the caller is responsible for
        // making only one instance of RealSystem in the process.
        match unsafe { nix::unistd::fork()? } {
            Parent { child } => Ok(Box::new(move |_env, _task| {
                Box::pin(async move { child.into() })
            })),
            Child => Ok(Box::new(|env, task| {
                Box::pin(async move {
                    task(env).await;
                    std::process::exit(env.exit_status.0)
                })
            })),
        }
    }

    fn wait(&mut self, target: Pid) -> Result<Option<(Pid, ProcessState)>> {
        use nix::sys::wait::WaitPidFlag;
        let options = WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED | WaitPidFlag::WNOHANG;
        let status = nix::sys::wait::waitpid(Some(target.into()), options.into())?;
        let result = match status {
            WaitStatus::Continued(pid) => Some((pid.into(), ProcessState::Running)),
            WaitStatus::Exited(pid, exit_status) => {
                Some((pid.into(), ProcessState::Exited(ExitStatus(exit_status))))
            }
            WaitStatus::Stopped(pid, signal) => Some((pid.into(), ProcessState::Stopped(signal))),
            WaitStatus::Signaled(pid, signal, core_dump) => {
                Some((pid.into(), ProcessState::Signaled { signal, core_dump }))
            }
            _ => None,
        };
        Ok(result)
    }

    fn execve(&mut self, path: &CStr, args: &[CString], envs: &[CString]) -> Result<Infallible> {
        loop {
            // TODO Use Result::into_err
            let result = nix::unistd::execve(path, args, envs);
            if result != Err(NixErrno::EINTR) {
                return Ok(result?);
            }
        }
    }

    fn getcwd(&self) -> Result<PathBuf> {
        Ok(nix::unistd::getcwd()?)
    }

    fn chdir(&mut self, path: &CStr) -> Result<()> {
        nix::unistd::chdir(path)?;
        Ok(())
    }

    fn getpwnam_dir(&self, name: &str) -> Result<Option<PathBuf>> {
        let user = nix::unistd::User::from_name(name)?;
        Ok(user.map(|user| user.dir))
    }

    fn confstr_path(&self) -> Result<OsString> {
        // TODO Support other platforms
        #[cfg(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "watchos"
        ))]
        unsafe {
            use std::os::unix::ffi::OsStringExt as _;
            let size = nix::libc::confstr(nix::libc::_CS_PATH, std::ptr::null_mut(), 0);
            if size == 0 {
                return Err(Errno::last());
            }
            let mut buffer = Vec::<u8>::with_capacity(size);
            let final_size =
                nix::libc::confstr(nix::libc::_CS_PATH, buffer.as_mut_ptr() as *mut _, size);
            if final_size == 0 {
                return Err(Errno::last());
            }
            if final_size > size {
                return Err(Errno::ERANGE);
            }
            buffer.set_len(final_size - 1); // The last byte is a null terminator.
            return Ok(OsString::from_vec(buffer));
        }

        #[allow(unreachable_code)]
        Err(Errno::ENOSYS)
    }

    /// Returns the path to the shell.
    ///
    /// On Linux, this function returns `/proc/self/exe`. On other platforms, it
    /// searches for an executable `sh` from the default PATH returned by
    /// [`confstr_path`](Self::confstr_path).
    fn shell_path(&self) -> CString {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        if self.is_executable_file(CStr::from_bytes_with_nul(b"/proc/self/exe\0").unwrap()) {
            return CString::new("/proc/self/exe").unwrap();
        }
        // TODO Add optimization for other targets

        // Find an executable "sh" from the default PATH
        if let Ok(path) = self.confstr_path() {
            if let Some(full_path) = path
                .as_bytes()
                .split(|b| *b == b':')
                .map(|dir| PathBuf::from_iter([OsStr::from_bytes(dir), OsStr::from_bytes(b"sh")]))
                .filter(|full_path| full_path.is_absolute())
                .filter_map(|full_path| CString::new(full_path.into_os_string().into_vec()).ok())
                .find(|full_path| self.is_executable_file(full_path))
            {
                return full_path;
            }
        }

        // The last resort
        CString::new("/bin/sh").unwrap()
    }

    fn getrlimit(&self, resource: Resource) -> std::io::Result<LimitPair> {
        let raw_resource = resource
            .as_raw_type()
            .ok_or_else(|| std::io::Error::from_raw_os_error(nix::libc::EINVAL as _))?;

        let mut limits = MaybeUninit::<nix::libc::rlimit>::uninit();
        unsafe { nix::libc::getrlimit(raw_resource as _, limits.as_mut_ptr()) }.errno_if_m1()?;
        let limits = unsafe { limits.assume_init() };
        Ok(limits.into())
    }

    fn setrlimit(&mut self, resource: Resource, limits: LimitPair) -> std::io::Result<()> {
        let raw_resource = resource
            .as_raw_type()
            .ok_or_else(|| std::io::Error::from_raw_os_error(nix::libc::EINVAL as _))?;

        let limits = limits.into();
        unsafe { nix::libc::setrlimit(raw_resource as _, &limits) }.errno_if_m1()?;
        Ok(())
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
    fn next(&mut self) -> Result<Option<DirEntry>> {
        Errno::clear();
        let entry = unsafe { nix::libc::readdir(self.0.as_ptr()) };
        let errno = Errno::last();
        match NonNull::new(entry) {
            None if errno != Errno::NO_ERROR => Err(errno),
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

mod signal;

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
