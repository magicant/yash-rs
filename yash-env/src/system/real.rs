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
//!
//! This module is implemented on Unix-like targets only. It provides an
//! implementation of the `System` trait that interacts with the underlying
//! operating system. This implementation is intended to be used in a real
//! environment, such as a shell running on a Unix-like operating system.

mod errno;
mod file_system;
mod open_flag;
mod resource;
mod signal;

use super::resource::LimitPair;
use super::resource::Resource;
use super::ChildProcessStarter;
use super::Dir;
use super::DirEntry;
use super::Disposition;
#[cfg(doc)]
use super::Env;
use super::Errno;
use super::FdFlag;
use super::Gid;
use super::Mode;
use super::OfdAccess;
use super::OpenFlag;
use super::Result;
use super::SigmaskOp;
use super::Stat;
use super::System;
use super::Times;
use super::Uid;
use crate::io::Fd;
use crate::job::Pid;
use crate::job::ProcessResult;
use crate::job::ProcessState;
use crate::path::Path;
use crate::path::PathBuf;
use crate::str::UnixStr;
use crate::str::UnixString;
use enumset::EnumSet;
use nix::errno::Errno as NixErrno;
use nix::fcntl::AtFlags;
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
use std::future::Future;
use std::io::SeekFrom;
use std::mem::MaybeUninit;
use std::num::NonZeroI32;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::ffi::OsStringExt as _;
use std::os::unix::io::IntoRawFd;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::compiler_fence;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use yash_executor::Executor;

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

impl Pid {
    #[inline(always)]
    const fn to_nix(self) -> nix::unistd::Pid {
        nix::unistd::Pid::from_raw(self.0)
    }
    #[inline(always)]
    const fn from_nix(pid: nix::unistd::Pid) -> Self {
        Pid(pid.as_raw())
    }
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

/// Converts a `Duration` to a `timespec`.
///
/// The return value is a `MaybeUninit` because the `timespec` struct may have
/// padding or extension fields that are not initialized by this function.
#[must_use]
fn to_timespec(duration: Duration) -> MaybeUninit<nix::libc::timespec> {
    let seconds = duration
        .as_secs()
        .try_into()
        .unwrap_or(nix::libc::time_t::MAX);
    let mut timespec = MaybeUninit::<nix::libc::timespec>::uninit();
    unsafe {
        (&raw mut (*timespec.as_mut_ptr()).tv_sec).write(seconds);
        (&raw mut (*timespec.as_mut_ptr()).tv_nsec).write(duration.subsec_nanos() as _);
    }
    timespec
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
    fn fstat(&self, fd: Fd) -> Result<Stat> {
        let mut stat = MaybeUninit::<nix::libc::stat>::uninit();
        unsafe { nix::libc::fstat(fd.0, stat.as_mut_ptr()) }.errno_if_m1()?;
        Ok(Stat::from_raw(&stat))
    }

    fn fstatat(&self, dir_fd: Fd, path: &CStr, follow_symlinks: bool) -> Result<Stat> {
        let flags = if follow_symlinks {
            0
        } else {
            nix::libc::AT_SYMLINK_NOFOLLOW
        };

        let mut stat = MaybeUninit::<nix::libc::stat>::uninit();
        unsafe { nix::libc::fstatat(dir_fd.0, path.as_ptr(), stat.as_mut_ptr(), flags) }
            .errno_if_m1()?;
        Ok(Stat::from_raw(&stat))
    }

    fn is_executable_file(&self, path: &CStr) -> bool {
        is_regular_file(path) && is_executable(path)
    }

    fn is_directory(&self, path: &CStr) -> bool {
        is_directory(path)
    }

    fn pipe(&mut self) -> Result<(Fd, Fd)> {
        let mut fds = MaybeUninit::<[c_int; 2]>::uninit();
        // TODO Use as_mut_ptr rather than cast when array_ptr_get is stabilized
        unsafe { nix::libc::pipe(fds.as_mut_ptr().cast()) }.errno_if_m1()?;
        let fds = unsafe { fds.assume_init() };
        Ok((Fd(fds[0]), Fd(fds[1])))
    }

    fn dup(&mut self, from: Fd, to_min: Fd, flags: EnumSet<FdFlag>) -> Result<Fd> {
        let command = if flags.contains(FdFlag::CloseOnExec) {
            nix::libc::F_DUPFD_CLOEXEC
        } else {
            nix::libc::F_DUPFD
        };
        unsafe { nix::libc::fcntl(from.0, command, to_min.0) }
            .errno_if_m1()
            .map(Fd)
    }

    fn dup2(&mut self, from: Fd, to: Fd) -> Result<Fd> {
        loop {
            let result = unsafe { nix::libc::dup2(from.0, to.0) }
                .errno_if_m1()
                .map(Fd);
            if result != Err(Errno::EINTR) {
                return result;
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
        let parent_dir = OsStr::from_bytes(parent_dir.as_unix_str().as_bytes());
        let file = tempfile::tempfile_in(parent_dir)
            .map_err(|errno| Errno(errno.raw_os_error().unwrap_or(0)))?;
        let fd = Fd(file.into_raw_fd());

        // Clear the CLOEXEC flag
        _ = self.fcntl_setfd(fd, EnumSet::empty());

        Ok(fd)
    }

    fn close(&mut self, fd: Fd) -> Result<()> {
        loop {
            let result = unsafe { nix::libc::close(fd.0) }.errno_if_m1().map(drop);
            match result {
                Err(Errno::EBADF) => return Ok(()),
                Err(Errno::EINTR) => continue,
                other => return other,
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

    fn fcntl_getfd(&self, fd: Fd) -> Result<EnumSet<FdFlag>> {
        let bits = unsafe { nix::libc::fcntl(fd.0, nix::libc::F_GETFD) }.errno_if_m1()?;
        let mut flags = EnumSet::empty();
        if bits & nix::libc::FD_CLOEXEC != 0 {
            flags.insert(FdFlag::CloseOnExec);
        }
        Ok(flags)
    }

    fn fcntl_setfd(&mut self, fd: Fd, flags: EnumSet<FdFlag>) -> Result<()> {
        let mut bits = 0 as c_int;
        if flags.contains(FdFlag::CloseOnExec) {
            bits |= nix::libc::FD_CLOEXEC;
        }
        unsafe { nix::libc::fcntl(fd.0, nix::libc::F_SETFD, bits) }
            .errno_if_m1()
            .map(drop)
    }

    fn isatty(&self, fd: Fd) -> bool {
        (unsafe { nix::libc::isatty(fd.0) } != 0)
    }

    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> Result<usize> {
        loop {
            let result = unsafe { nix::libc::read(fd.0, buffer.as_mut_ptr().cast(), buffer.len()) }
                .errno_if_m1();
            if result != Err(Errno::EINTR) {
                return Ok(result?.try_into().unwrap());
            }
        }
    }

    fn write(&mut self, fd: Fd, buffer: &[u8]) -> Result<usize> {
        loop {
            let result = unsafe { nix::libc::write(fd.0, buffer.as_ptr().cast(), buffer.len()) }
                .errno_if_m1();
            if result != Err(Errno::EINTR) {
                return Ok(result?.try_into().unwrap());
            }
        }
    }

    fn lseek(&mut self, fd: Fd, position: SeekFrom) -> Result<u64> {
        let (offset, whence) = match position {
            SeekFrom::Start(offset) => {
                let offset = offset.try_into().map_err(|_| Errno::EOVERFLOW)?;
                (offset, nix::libc::SEEK_SET)
            }
            SeekFrom::End(offset) => (offset, nix::libc::SEEK_END),
            SeekFrom::Current(offset) => (offset, nix::libc::SEEK_CUR),
        };
        let new_offset = unsafe { nix::libc::lseek(fd.0, offset, whence) }.errno_if_m1()?;
        Ok(new_offset.try_into().unwrap())
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
        let utime = unsafe { (&raw const (*tms.as_ptr()).tms_utime).read() };
        let stime = unsafe { (&raw const (*tms.as_ptr()).tms_stime).read() };
        let cutime = unsafe { (&raw const (*tms.as_ptr()).tms_cutime).read() };
        let cstime = unsafe { (&raw const (*tms.as_ptr()).tms_cstime).read() };

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
        op: Option<(SigmaskOp, &[signal::Number])>,
        old_mask: Option<&mut Vec<signal::Number>>,
    ) -> Result<()> {
        unsafe {
            let (how, raw_mask) = match op {
                None => (nix::libc::SIG_BLOCK, None),
                Some((op, mask)) => {
                    let how = match op {
                        SigmaskOp::Add => nix::libc::SIG_BLOCK,
                        SigmaskOp::Remove => nix::libc::SIG_UNBLOCK,
                        SigmaskOp::Set => nix::libc::SIG_SETMASK,
                    };

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
            let result = nix::libc::sigprocmask(how, raw_set_ptr, raw_old_set_ptr);
            result.errno_if_m1().map(drop)?;

            if let Some((old_mask, raw_old_mask)) = old_mask_pair {
                old_mask.clear();
                signal::sigset_to_vec(raw_old_mask.as_ptr(), old_mask);
            }

            Ok(())
        }
    }

    fn sigaction(&mut self, signal: signal::Number, handling: Disposition) -> Result<Disposition> {
        unsafe {
            let new_action = handling.to_sigaction();

            let mut old_action = MaybeUninit::<nix::libc::sigaction>::uninit();
            let old_mask_ptr = &raw mut (*old_action.as_mut_ptr()).sa_mask;
            // POSIX requires *all* sigset_t objects to be initialized before use.
            nix::libc::sigemptyset(old_mask_ptr).errno_if_m1()?;

            nix::libc::sigaction(
                signal.as_raw(),
                new_action.as_ptr(),
                old_action.as_mut_ptr(),
            )
            .errno_if_m1()?;

            let old_handling = Disposition::from_sigaction(&old_action);
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
        readers: &mut Vec<Fd>,
        writers: &mut Vec<Fd>,
        timeout: Option<Duration>,
        signal_mask: Option<&[signal::Number]>,
    ) -> Result<c_int> {
        use std::ptr::{null, null_mut};

        let max_fd = readers.iter().chain(writers.iter()).max();
        let nfds = max_fd
            .map(|fd| fd.0.checked_add(1).ok_or(Errno::EBADF))
            .transpose()?
            .unwrap_or(0);

        fn to_raw_fd_set(fds: &[Fd]) -> MaybeUninit<nix::libc::fd_set> {
            let mut raw_fds = MaybeUninit::<nix::libc::fd_set>::uninit();
            unsafe {
                nix::libc::FD_ZERO(raw_fds.as_mut_ptr());
                for fd in fds {
                    nix::libc::FD_SET(fd.0, raw_fds.as_mut_ptr());
                }
            }
            raw_fds
        }
        let mut raw_readers = to_raw_fd_set(readers);
        let mut raw_writers = to_raw_fd_set(writers);
        let readers_ptr = raw_readers.as_mut_ptr();
        let writers_ptr = raw_writers.as_mut_ptr();
        let errors = null_mut();

        let timeout_spec = to_timespec(timeout.unwrap_or_default());
        let timeout_ptr = if timeout.is_some() {
            timeout_spec.as_ptr()
        } else {
            null()
        };

        let mut raw_mask = MaybeUninit::<nix::libc::sigset_t>::uninit();
        let raw_mask_ptr = match signal_mask {
            None => null(),
            Some(signal_mask) => {
                unsafe { nix::libc::sigemptyset(raw_mask.as_mut_ptr()) }.errno_if_m1()?;
                for &signal in signal_mask {
                    unsafe { nix::libc::sigaddset(raw_mask.as_mut_ptr(), signal.as_raw()) }
                        .errno_if_m1()?;
                }
                raw_mask.as_ptr()
            }
        };

        let count = unsafe {
            nix::libc::pselect(
                nfds,
                readers_ptr,
                writers_ptr,
                errors,
                timeout_ptr,
                raw_mask_ptr,
            )
        }
        .errno_if_m1()?;

        readers.retain(|fd| unsafe { nix::libc::FD_ISSET(fd.0, readers_ptr) });
        writers.retain(|fd| unsafe { nix::libc::FD_ISSET(fd.0, writers_ptr) });

        Ok(count)
    }

    fn getpid(&self) -> Pid {
        Pid(unsafe { nix::libc::getpid() })
    }

    fn getppid(&self) -> Pid {
        Pid(unsafe { nix::libc::getppid() })
    }

    fn getpgrp(&self) -> Pid {
        Pid(unsafe { nix::libc::getpgrp() })
    }

    fn setpgid(&mut self, pid: Pid, pgid: Pid) -> Result<()> {
        let result = unsafe { nix::libc::setpgid(pid.0, pgid.0) };
        result.errno_if_m1().map(drop)
    }

    fn tcgetpgrp(&self, fd: Fd) -> Result<Pid> {
        unsafe { nix::libc::tcgetpgrp(fd.0) }.errno_if_m1().map(Pid)
    }

    fn tcsetpgrp(&mut self, fd: Fd, pgid: Pid) -> Result<()> {
        let result = unsafe { nix::libc::tcsetpgrp(fd.0, pgid.0) };
        result.errno_if_m1().map(drop)
    }

    /// Creates a new child process.
    ///
    /// This implementation calls the `fork` system call and returns both in the
    /// parent and child process. In the parent, the returned
    /// `ChildProcessStarter` ignores any arguments and returns the child
    /// process ID. In the child, the starter runs the task and exits the
    /// process.
    fn new_child_process(&mut self) -> Result<ChildProcessStarter> {
        let raw_pid = unsafe { nix::libc::fork() }.errno_if_m1()?;
        if raw_pid != 0 {
            // Parent process
            return Ok(Box::new(move |_env, _task| Pid(raw_pid)));
        }

        // Child process
        Ok(Box::new(|env, task| {
            let system = env.system.clone();
            // Here we create a new executor to run the task. This makes sure that any
            // other concurrent tasks in the parent process do not interfere with the
            // child process.
            let executor = Executor::new();
            let task = Box::pin(async move {
                task(env).await;
                std::process::exit(env.exit_status.0)
            });
            // SAFETY: We never create new threads in the whole process, so wakers are
            // never shared between threads.
            unsafe { executor.spawn_pinned(task) }
            loop {
                executor.run_until_stalled();
                system.select(false).ok();
            }
        }))
    }

    fn wait(&mut self, target: Pid) -> Result<Option<(Pid, ProcessState)>> {
        use nix::sys::wait::{WaitPidFlag, WaitStatus::*};
        let options = WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED | WaitPidFlag::WNOHANG;
        let status = nix::sys::wait::waitpid(Some(target.to_nix()), options.into())?;
        match status {
            StillAlive => Ok(None),
            Continued(pid) => Ok(Some((Pid::from_nix(pid), ProcessState::Running))),
            Exited(pid, exit_status) => Ok(Some((
                Pid::from_nix(pid),
                ProcessState::exited(exit_status),
            ))),
            Signaled(pid, signal, core_dump) => {
                // SAFETY: The signal number is always a valid signal number, which is non-zero.
                let raw_number = unsafe { NonZeroI32::new_unchecked(signal as _) };
                let signal = signal::Number::from_raw_unchecked(raw_number);
                let process_result = ProcessResult::Signaled { signal, core_dump };
                Ok(Some((Pid::from_nix(pid), process_result.into())))
            }
            Stopped(pid, signal) => {
                // SAFETY: The signal number is always a valid signal number, which is non-zero.
                let raw_number = unsafe { NonZeroI32::new_unchecked(signal as _) };
                let signal = signal::Number::from_raw_unchecked(raw_number);
                Ok(Some((Pid::from_nix(pid), ProcessState::stopped(signal))))
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
        let path = nix::unistd::getcwd()?;
        let raw = path.into_os_string().into_vec();
        Ok(PathBuf::from(UnixString::from_vec(raw)))
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
        Ok(user.map(|user| {
            let dir = user.dir.into_os_string().into_vec();
            PathBuf::from(UnixString::from_vec(dir))
        }))
    }

    fn confstr_path(&self) -> Result<UnixString> {
        // TODO Support other platforms
        #[cfg(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "watchos"
        ))]
        unsafe {
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
            return Ok(UnixString::from_vec(buffer));
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
                .map(|dir| Path::new(UnixStr::from_bytes(dir)).join("sh"))
                .filter(|full_path| full_path.is_absolute())
                .filter_map(|full_path| CString::new(full_path.into_unix_string().into_vec()).ok())
                .find(|full_path| self.is_executable_file(full_path))
            {
                return full_path;
            }
        }

        // The last resort
        c"/bin/sh".to_owned()
    }

    fn getrlimit(&self, resource: Resource) -> Result<LimitPair> {
        let raw_resource = resource.as_raw_type().ok_or(Errno::EINVAL)?;

        let mut limits = MaybeUninit::<nix::libc::rlimit>::uninit();
        unsafe { nix::libc::getrlimit(raw_resource as _, limits.as_mut_ptr()) }.errno_if_m1()?;
        Ok(LimitPair {
            soft: unsafe { (&raw const (*limits.as_ptr()).rlim_cur).read() },
            hard: unsafe { (&raw const (*limits.as_ptr()).rlim_max).read() },
        })
    }

    fn setrlimit(&mut self, resource: Resource, limits: LimitPair) -> Result<()> {
        let raw_resource = resource.as_raw_type().ok_or(Errno::EINVAL)?;

        let mut rlimit = MaybeUninit::<nix::libc::rlimit>::uninit();
        unsafe {
            (&raw mut (*rlimit.as_mut_ptr()).rlim_cur).write(limits.soft);
            (&raw mut (*rlimit.as_mut_ptr()).rlim_max).write(limits.hard);
        }

        unsafe { nix::libc::setrlimit(raw_resource as _, rlimit.as_ptr()) }.errno_if_m1()?;
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
        if entry.is_null() {
            if errno == Errno::NO_ERROR {
                Ok(None)
            } else {
                Err(errno)
            }
        } else {
            // TODO Use as_ptr rather than cast when array_ptr_get is stabilized
            let name = unsafe { CStr::from_ptr((&raw const (*entry).d_name).cast()) };
            let name = UnixStr::from_bytes(name.to_bytes());
            Ok(Some(DirEntry { name }))
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
