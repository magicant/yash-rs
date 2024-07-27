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

mod open_flag;
mod signal;

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
use super::Gid;
use super::Mode;
use super::OfdAccess;
use super::OpenFlag;
use super::Result;
use super::SigmaskHow;
use super::System;
use super::TimeSpec;
use super::Times;
use super::Uid;
use crate::io::Fd;
use crate::job::Pid;
use crate::job::ProcessResult;
use crate::job::ProcessState;
use crate::SignalHandling;
use enumset::EnumSet;
use nix::errno::Errno as NixErrno;
use nix::libc::DIR;
use nix::libc::{S_IFDIR, S_IFMT, S_IFREG};
use nix::sys::stat::stat;
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
use std::num::NonZeroI32;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt as _;
use std::os::unix::io::IntoRawFd;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::ptr::addr_of;
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
static CAUGHT_SIGNALS: [AtomicIsize; 8] = [const { AtomicIsize::new(0) }; 8];

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

    fn open(
        &mut self,
        path: &CStr,
        access: OfdAccess,
        flags: EnumSet<OpenFlag>,
        mode: Mode,
    ) -> Result<Fd> {
        let mut raw_flags = access.to_real_flags().ok_or(Errno::EINVAL)?;
        for flag in flags {
            raw_flags |= flag.to_real_flags().ok_or(Errno::EINVAL)?;
        }

        #[cfg(not(target_os = "redox"))]
        let mode_bits = mode.bits() as std::ffi::c_uint;
        #[cfg(target_os = "redox")]
        let mode_bits = mode.bits() as c_int;

        unsafe { nix::libc::open(path.as_ptr(), raw_flags, mode_bits) }
            .errno_if_m1()
            .map(Fd)
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

    fn ofd_access(&self, fd: Fd) -> Result<OfdAccess> {
        let flags = unsafe { nix::libc::fcntl(fd.0, nix::libc::F_GETFL) }.errno_if_m1()?;
        Ok(OfdAccess::from_real_flags(flags))
    }

    fn get_and_set_nonblocking(&mut self, fd: Fd, nonblocking: bool) -> Result<bool> {
        let old_flags = unsafe { nix::libc::fcntl(fd.0, nix::libc::F_GETFL) }.errno_if_m1()?;
        let new_flags = if nonblocking {
            old_flags | nix::libc::O_NONBLOCK
        } else {
            old_flags & !nix::libc::O_NONBLOCK
        };
        if new_flags != old_flags {
            unsafe { nix::libc::fcntl(fd.0, nix::libc::F_SETFL, new_flags) }.errno_if_m1()?;
        }
        let was_nonblocking = old_flags & nix::libc::O_NONBLOCK != 0;
        Ok(was_nonblocking)
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

    fn umask(&mut self, new_mask: Mode) -> Mode {
        Mode::from_bits_retain(unsafe { nix::libc::umask(new_mask.bits()) })
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

        let ticks_per_second = unsafe { nix::libc::sysconf(nix::libc::_SC_CLK_TCK) };
        if ticks_per_second <= 0 {
            return Err(Errno::last());
        }

        // SAFETY: The four fields of `tms` have been initialized by `times`.
        // (But that does not mean *all* fields are initialized,
        // so we cannot use `assume_init` here.)
        let utime = unsafe { addr_of!((*tms.as_ptr()).tms_utime).read() };
        let stime = unsafe { addr_of!((*tms.as_ptr()).tms_stime).read() };
        let cutime = unsafe { addr_of!((*tms.as_ptr()).tms_cutime).read() };
        let cstime = unsafe { addr_of!((*tms.as_ptr()).tms_cstime).read() };

        Ok(Times {
            self_user: utime as f64 / ticks_per_second as f64,
            self_system: stime as f64 / ticks_per_second as f64,
            children_user: cutime as f64 / ticks_per_second as f64,
            children_system: cstime as f64 / ticks_per_second as f64,
        })
    }

    fn validate_signal(&self, number: signal::RawNumber) -> Option<(signal::Name, signal::Number)> {
        let non_zero = NonZeroI32::new(number)?;
        let name = signal::Name::try_from_raw_real(number)?;
        Some((name, signal::Number::from_raw_unchecked(non_zero)))
    }

    #[inline(always)]
    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number> {
        name.to_raw_real()
    }

    fn sigmask(
        &mut self,
        op: Option<(SigmaskHow, &[signal::Number])>,
        old_mask: Option<&mut Vec<signal::Number>>,
    ) -> Result<()> {
        unsafe {
            let (how, raw_mask) = match op {
                None => (SigmaskHow::SIG_BLOCK, None),
                Some((how, mask)) => {
                    let mut raw_mask = MaybeUninit::<nix::libc::sigset_t>::uninit();
                    nix::libc::sigemptyset(raw_mask.as_mut_ptr()).errno_if_m1()?;
                    for &signal in mask {
                        nix::libc::sigaddset(raw_mask.as_mut_ptr(), signal.as_raw())
                            .errno_if_m1()?;
                    }
                    (how, Some(raw_mask))
                }
            };
            let mut old_mask_pair = match old_mask {
                None => None,
                Some(old_mask) => {
                    let mut raw_old_mask = MaybeUninit::<nix::libc::sigset_t>::uninit();
                    // POSIX requires *all* sigset_t objects to be initialized before use.
                    nix::libc::sigemptyset(raw_old_mask.as_mut_ptr()).errno_if_m1()?;
                    Some((old_mask, raw_old_mask))
                }
            };

            let raw_set_ptr = raw_mask
                .as_ref()
                .map_or(std::ptr::null(), |raw_set| raw_set.as_ptr());
            let raw_old_set_ptr = old_mask_pair
                .as_mut()
                .map_or(std::ptr::null_mut(), |(_, raw_old_mask)| {
                    raw_old_mask.as_mut_ptr()
                });
            let result = nix::libc::sigprocmask(how as _, raw_set_ptr, raw_old_set_ptr);
            result.errno_if_m1().map(drop)?;

            if let Some((old_mask, raw_old_mask)) = old_mask_pair {
                old_mask.clear();
                signal::sigset_to_vec(raw_old_mask.as_ptr(), old_mask);
            }

            Ok(())
        }
    }

    fn sigaction(
        &mut self,
        signal: signal::Number,
        handling: SignalHandling,
    ) -> Result<SignalHandling> {
        unsafe {
            let new_action = handling.to_sigaction();

            let mut old_action = MaybeUninit::<nix::libc::sigaction>::uninit();
            let old_mask_ptr = std::ptr::addr_of_mut!((*old_action.as_mut_ptr()).sa_mask);
            // POSIX requires *all* sigset_t objects to be initialized before use.
            nix::libc::sigemptyset(old_mask_ptr).errno_if_m1()?;

            nix::libc::sigaction(
                signal.as_raw(),
                new_action.as_ptr(),
                old_action.as_mut_ptr(),
            )
            .errno_if_m1()?;

            let old_handling = SignalHandling::from_sigaction(&old_action);
            Ok(old_handling)
        }
    }

    fn caught_signals(&mut self) -> Vec<signal::Number> {
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

            if let Some((_name, number)) = self.validate_signal(signal as signal::RawNumber) {
                signals.push(number)
            } else {
                // ignore unknown signal
            }
        }
        signals
    }

    fn kill(
        &mut self,
        target: Pid,
        signal: Option<signal::Number>,
    ) -> Pin<Box<(dyn Future<Output = Result<()>>)>> {
        Box::pin(async move {
            let raw = signal.map_or(0, signal::Number::as_raw);
            unsafe { nix::libc::kill(target.0, raw) }.errno_if_m1()?;
            Ok(())
        })
    }

    fn select(
        &mut self,
        readers: &mut FdSet,
        writers: &mut FdSet,
        timeout: Option<&TimeSpec>,
        signal_mask: Option<&[signal::Number]>,
    ) -> Result<c_int> {
        use std::ptr::{null, null_mut};
        let nfds = readers.upper_bound().max(writers.upper_bound()).0;
        let readers = &mut readers.inner;
        let writers = &mut writers.inner;
        let errors = null_mut();
        let timeout = timeout.map_or(null(), |timeout| timeout.as_ref());

        let raw_mask = match signal_mask {
            None => None,
            Some(mask) => {
                let mut raw_mask = MaybeUninit::<nix::libc::sigset_t>::uninit();
                unsafe { nix::libc::sigemptyset(raw_mask.as_mut_ptr()) }.errno_if_m1()?;
                for &signal in mask {
                    unsafe { nix::libc::sigaddset(raw_mask.as_mut_ptr(), signal.as_raw()) }
                        .errno_if_m1()?;
                }
                Some(raw_mask)
            }
        };

        let raw_mask_ptr = raw_mask
            .as_ref()
            .map_or(null(), |raw_mask| raw_mask.as_ptr());
        unsafe { nix::libc::pselect(nfds, readers, writers, errors, timeout, raw_mask_ptr) }
            .errno_if_m1()
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
        use nix::sys::wait::{WaitPidFlag, WaitStatus::*};
        let options = WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED | WaitPidFlag::WNOHANG;
        let status = nix::sys::wait::waitpid(Some(target.into()), options.into())?;
        match status {
            StillAlive => Ok(None),
            Continued(pid) => Ok(Some((pid.into(), ProcessState::Running))),
            Exited(pid, exit_status) => Ok(Some((pid.into(), ProcessState::exited(exit_status)))),
            Signaled(pid, signal, core_dump) => {
                // SAFETY: The signal number is always a valid signal number, which is non-zero.
                let raw_number = unsafe { NonZeroI32::new_unchecked(signal as _) };
                let signal = signal::Number::from_raw_unchecked(raw_number);
                let process_result = ProcessResult::Signaled { signal, core_dump };
                Ok(Some((pid.into(), process_result.into())))
            }
            Stopped(pid, signal) => {
                // SAFETY: The signal number is always a valid signal number, which is non-zero.
                let raw_number = unsafe { NonZeroI32::new_unchecked(signal as _) };
                let signal = signal::Number::from_raw_unchecked(raw_number);
                Ok(Some((pid.into(), ProcessState::stopped(signal))))
            }
            #[allow(unreachable_patterns)]
            _ => unreachable!(),
        }
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

    fn getuid(&self) -> Uid {
        Uid(unsafe { nix::libc::getuid() })
    }

    fn geteuid(&self) -> Uid {
        Uid(unsafe { nix::libc::geteuid() })
    }

    fn getgid(&self) -> Gid {
        Gid(unsafe { nix::libc::getgid() })
    }

    fn getegid(&self) -> Gid {
        Gid(unsafe { nix::libc::getegid() })
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
        if self.is_executable_file(c"/proc/self/exe") {
            return c"/proc/self/exe".to_owned();
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
        c"/bin/sh".to_owned()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_system_directory_entries() {
        let mut system = unsafe { RealSystem::new() };
        let mut dir = system.opendir(c".").unwrap();
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

            catch_signal(nix::libc::SIGINT);
            catch_signal(nix::libc::SIGTERM);
            catch_signal(nix::libc::SIGTERM);
            catch_signal(nix::libc::SIGCHLD);

            let sigint =
                signal::Number::from_raw_unchecked(NonZeroI32::new(nix::libc::SIGINT).unwrap());
            let sigterm =
                signal::Number::from_raw_unchecked(NonZeroI32::new(nix::libc::SIGTERM).unwrap());
            let sigchld =
                signal::Number::from_raw_unchecked(NonZeroI32::new(nix::libc::SIGCHLD).unwrap());

            let result = system.caught_signals();
            assert_eq!(result, [sigint, sigterm, sigchld]);
            let result = system.caught_signals();
            assert_eq!(result, []);
        }
    }
}
