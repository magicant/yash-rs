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
//!
//! This module also defines elements that compose a virtual system.
//!
//! # File system
//!
//! Currently, only regular files and directories are supported.
//!
//! Pathname resolution is not yet fully simulated. Especially, symbolic links
//! and the `.` and `..` components are not supported.
//!
//! # Processes
//!
//! A virtual system initially has one process, but can have more processes as a
//! result of simulating fork. Each process has its own state.
//!
//! # I/O
//!
//! Currently, read and write operations on files and unnamed pipes are
//! supported.
//!
//! # Signals
//!
//! The virtual system can simulate sending signals to processes. Processes can
//! block, ignore, and catch signals.

mod file_system;
mod io;
mod process;
mod signal;

pub use self::file_system::*;
pub use self::io::*;
pub use self::process::*;
pub use self::signal::*;
use super::resource::LimitPair;
use super::resource::Resource;
use super::resource::RLIM_INFINITY;
use super::AtFlags;
use super::Dir;
use super::Errno;
use super::FdFlag;
use super::FdSet;
use super::FileStat;
use super::Gid;
use super::OfdAccess;
use super::OpenFlag;
use super::Result;
use super::SigmaskHow;
use super::TimeSpec;
use super::Times;
use super::Uid;
use super::AT_FDCWD;
use crate::io::Fd;
use crate::job::Pid;
use crate::job::ProcessState;
use crate::system::ChildProcessStarter;
use crate::SignalHandling;
use crate::System;
use enumset::EnumSet;
use nix::sys::stat::SFlag;
use std::borrow::Cow;
use std::cell::Cell;
use std::cell::Ref;
use std::cell::RefCell;
use std::cell::RefMut;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::convert::TryInto;
use std::ffi::c_int;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fmt::Debug;
use std::future::poll_fn;
use std::future::Future;
use std::io::SeekFrom;
use std::mem::MaybeUninit;
use std::num::NonZeroI32;
use std::ops::DerefMut as _;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use std::time::Duration;
use std::time::Instant;

/// Simulated system.
///
/// See the [module-level documentation](self) to grasp a basic understanding of
/// `VirtualSystem`.
///
/// A `VirtualSystem` instance has two members: `state` and `process_id`. The
/// former is a [`SystemState`] that effectively contains the state of the
/// system. The state is contained in `Rc` so that processes can share the same
/// state. The latter is a process ID that identifies a process calling the
/// [`System`] interface.
///
/// When you clone a virtual system, the clone will have the same `process_id`
/// as the original. To simulate the `fork` system call, you would probably want
/// to assign a new process ID and add a new [`Process`] to the system state.
#[derive(Clone, Debug)]
pub struct VirtualSystem {
    /// State of the system.
    pub state: Rc<RefCell<SystemState>>,

    /// Process ID of the process that is interacting with the system.
    pub process_id: Pid,
}

impl VirtualSystem {
    /// Creates a virtual system with an almost empty state.
    ///
    /// The `process_id` of the returned `VirtualSystem` will be 2.
    /// (Process ID 1 has special meaning in some system calls, so we don't use
    /// it as a default value.)
    ///
    /// The `state` of the returned `VirtualSystem` will have a [`Process`] with
    /// process ID 2 in the process set ([`SystemState::processes`]). The file
    /// system will contain files named `/dev/stdin`, `/dev/stdout`, and
    /// `/dev/stderr` that are opened in the process with file descriptor 0, 1,
    /// and 2, respectively. The file system also contains an empty directory
    /// `/tmp`.
    pub fn new() -> VirtualSystem {
        let mut state = SystemState::default();
        let mut process = Process::with_parent_and_group(Pid(1), Pid(1));

        let mut set_std_fd = |path, fd| {
            let file = Rc::new(RefCell::new(INode::new([])));
            state.file_system.save(path, Rc::clone(&file)).unwrap();
            let body = FdBody {
                open_file_description: Rc::new(RefCell::new(OpenFileDescription {
                    file,
                    offset: 0,
                    is_readable: true,
                    is_writable: true,
                    is_appending: true,
                })),
                flag: FdFlag::empty(),
            };
            process.set_fd(fd, body).unwrap();
        };
        set_std_fd("/dev/stdin", Fd::STDIN);
        set_std_fd("/dev/stdout", Fd::STDOUT);
        set_std_fd("/dev/stderr", Fd::STDERR);

        state
            .file_system
            .save(
                "/tmp",
                Rc::new(RefCell::new(INode {
                    body: FileBody::Directory {
                        files: Default::default(),
                    },
                    permissions: Mode::ALL_9,
                })),
            )
            .unwrap();

        let process_id = Pid(2);
        state.processes.insert(process_id, process);

        let state = Rc::new(RefCell::new(state));
        VirtualSystem { state, process_id }
    }

    /// Finds the current process from the system state.
    ///
    /// # Panics
    ///
    /// This function will panic if it cannot find a process having
    /// `self.process_id`.
    pub fn current_process(&self) -> Ref<'_, Process> {
        Ref::map(self.state.borrow(), |state| {
            &state.processes[&self.process_id]
        })
    }

    /// Finds the current process from the system state.
    ///
    /// # Panics
    ///
    /// This function will panic if it cannot find a process having
    /// `self.process_id`.
    pub fn current_process_mut(&mut self) -> RefMut<'_, Process> {
        RefMut::map(self.state.borrow_mut(), |state| {
            state.processes.get_mut(&self.process_id).unwrap()
        })
    }

    /// Calls the given closure passing the open file description for the FD.
    ///
    /// Returns `Err(Errno::EBADF)` if the FD is not open.
    pub fn with_open_file_description<F, R>(&self, fd: Fd, f: F) -> Result<R>
    where
        F: FnOnce(&OpenFileDescription) -> Result<R>,
    {
        let process = self.current_process();
        let body = process.get_fd(fd).ok_or(Errno::EBADF)?;
        let ofd = body.open_file_description.borrow();
        f(&ofd)
    }

    /// Calls the given closure passing the open file description for the FD.
    ///
    /// Returns `Err(Errno::EBADF)` if the FD is not open.
    pub fn with_open_file_description_mut<F, R>(&mut self, fd: Fd, f: F) -> Result<R>
    where
        F: FnOnce(&mut OpenFileDescription) -> Result<R>,
    {
        let mut process = self.current_process_mut();
        let body = process.get_fd_mut(fd).ok_or(Errno::EBADF)?;
        let mut ofd = body.open_file_description.borrow_mut();
        f(&mut ofd)
    }

    fn resolve_relative_path<'a>(&self, path: &'a Path) -> Cow<'a, Path> {
        if path.is_absolute() {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(self.current_process().cwd.join(path))
        }
    }

    fn resolve_existing_file(
        &self,
        _dir_fd: Fd,
        path: &Path,
        flags: AtFlags,
    ) -> Result<Rc<RefCell<INode>>> {
        // TODO Resolve relative to dir_fd
        // TODO Support AT_FDCWD
        const _POSIX_SYMLOOP_MAX: i32 = 8;

        let mut path = Cow::Borrowed(path);
        for _count in 0.._POSIX_SYMLOOP_MAX {
            let resolved_path = self.resolve_relative_path(&path);
            let inode = self.state.borrow().file_system.get(&resolved_path)?;
            if flags.contains(AtFlags::AT_SYMLINK_NOFOLLOW) {
                return Ok(inode);
            }

            let inode_ref = inode.borrow();
            if let FileBody::Symlink { target } = &inode_ref.body {
                let mut new_path = resolved_path.into_owned();
                new_path.pop();
                new_path.push(target);
                path = Cow::Owned(new_path);
            } else {
                drop(inode_ref);
                return Ok(inode);
            }
        }

        Err(Errno::ELOOP)
    }

    /// Blocks the calling thread until the current process is running.
    async fn block_until_running(&self) {
        let waker = Rc::new(Cell::new(None));

        poll_fn(|cx| {
            let mut state = self.state.borrow_mut();
            let Some(process) = state.processes.get_mut(&self.process_id) else {
                return Poll::Ready(());
            };

            match process.state {
                ProcessState::Running => Poll::Ready(()),
                ProcessState::Halted(result) => {
                    if result.is_stopped() {
                        waker.set(Some(cx.waker().clone()));
                        process.wake_on_resumption(Rc::downgrade(&waker));
                    }
                    Poll::Pending
                }
            }
        })
        .await
    }
}

impl Default for VirtualSystem {
    fn default() -> Self {
        VirtualSystem::new()
    }
}

fn stat(inode: &INode) -> Result<FileStat> {
    let (type_flag, size) = match &inode.body {
        FileBody::Regular { content, .. } => (SFlag::S_IFREG, content.len()),
        FileBody::Directory { files } => (SFlag::S_IFDIR, files.len()),
        FileBody::Fifo { content, .. } => (SFlag::S_IFIFO, content.len()),
        FileBody::Symlink { target } => (SFlag::S_IFLNK, target.as_os_str().len()),
    };
    let mut result: FileStat = unsafe { MaybeUninit::zeroed().assume_init() };
    result.st_mode = type_flag.bits() | inode.permissions.bits();
    result.st_size = size as _;
    result.st_dev = 1;
    result.st_ino = std::ptr::addr_of!(*inode) as nix::libc::ino_t;
    Ok(result)
}

impl System for VirtualSystem {
    /// Retrieves metadata of a file.
    ///
    /// The current implementation fills only the following values of the
    /// returned `FileStat`:
    ///
    /// - `st_mode`
    /// - `st_size`
    /// - `st_dev` (always 1)
    /// - `st_ino` (computed from the address of `INode`)
    fn fstat(&self, fd: Fd) -> Result<FileStat> {
        self.with_open_file_description(fd, |ofd| stat(&ofd.file.borrow()))
    }

    /// Retrieves metadata of a file.
    ///
    /// The current implementation fills only the following values of the
    /// returned `FileStat`:
    ///
    /// - `st_mode`
    /// - `st_size`
    /// - `st_dev` (always 1)
    /// - `st_ino` (computed from the address of `INode`)
    fn fstatat(&self, dir_fd: Fd, path: &CStr, flags: AtFlags) -> Result<FileStat> {
        let path = Path::new(OsStr::from_bytes(path.to_bytes()));
        let inode = self.resolve_existing_file(dir_fd, path, flags)?;
        let inode = inode.borrow();
        stat(&inode)
    }

    /// Tests whether the specified file is executable or not.
    ///
    /// The current implementation only checks if the file has any executable
    /// bit in the permissions. The file owner and group are not considered.
    fn is_executable_file(&self, path: &CStr) -> bool {
        let path = Path::new(OsStr::from_bytes(path.to_bytes()));
        let Ok(inode) = self.resolve_existing_file(AT_FDCWD, path, AtFlags::empty()) else {
            return false;
        };
        let inode = inode.borrow();
        inode.permissions.intersects(Mode::ALL_EXEC)
    }

    fn is_directory(&self, path: &CStr) -> bool {
        let path = Path::new(OsStr::from_bytes(path.to_bytes()));
        let Ok(inode) = self.resolve_existing_file(AT_FDCWD, path, AtFlags::empty()) else {
            return false;
        };
        let inode = inode.borrow();
        matches!(inode.body, FileBody::Directory { .. })
    }

    fn pipe(&mut self) -> Result<(Fd, Fd)> {
        let file = Rc::new(RefCell::new(INode {
            body: FileBody::Fifo {
                content: VecDeque::new(),
                readers: 1,
                writers: 1,
            },
            permissions: Mode::default(),
        }));
        let reader = OpenFileDescription {
            file: Rc::clone(&file),
            offset: 0,
            is_readable: true,
            is_writable: false,
            is_appending: false,
        };
        let writer = OpenFileDescription {
            file: Rc::clone(&file),
            offset: 0,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };

        let reader = FdBody {
            open_file_description: Rc::new(RefCell::new(reader)),
            flag: FdFlag::empty(),
        };
        let writer = FdBody {
            open_file_description: Rc::new(RefCell::new(writer)),
            flag: FdFlag::empty(),
        };

        let mut process = self.current_process_mut();
        let reader = process.open_fd(reader).map_err(|_| Errno::EMFILE)?;
        let writer = process.open_fd(writer).map_err(|_| {
            process.close_fd(reader);
            Errno::EMFILE
        })?;
        Ok((reader, writer))
    }

    fn dup(&mut self, from: Fd, to_min: Fd, flags: FdFlag) -> Result<Fd> {
        let mut process = self.current_process_mut();
        let mut body = process.fds.get(&from).ok_or(Errno::EBADF)?.clone();
        body.flag = flags;
        process.open_fd_ge(to_min, body).map_err(|_| Errno::EMFILE)
    }

    fn dup2(&mut self, from: Fd, to: Fd) -> Result<Fd> {
        let mut process = self.current_process_mut();
        let mut body = process.fds.get(&from).ok_or(Errno::EBADF)?.clone();
        body.flag = FdFlag::empty();
        process.set_fd(to, body).map_err(|_| Errno::EBADF)?;
        Ok(to)
    }

    fn open(
        &mut self,
        path: &CStr,
        access: OfdAccess,
        flags: EnumSet<OpenFlag>,
        mode: Mode,
    ) -> Result<Fd> {
        let path = self.resolve_relative_path(Path::new(OsStr::from_bytes(path.to_bytes())));
        let mut state = self.state.borrow_mut();
        let file = match state.file_system.get(&path) {
            Ok(inode) => {
                if flags.contains(OpenFlag::Exclusive) {
                    return Err(Errno::EEXIST);
                }
                if flags.contains(OpenFlag::Directory)
                    && !matches!(inode.borrow().body, FileBody::Directory { .. })
                {
                    return Err(Errno::ENOTDIR);
                }
                if flags.contains(OpenFlag::Truncate) {
                    if let FileBody::Regular { content, .. } = &mut inode.borrow_mut().body {
                        content.clear();
                    };
                }
                inode
            }
            Err(Errno::ENOENT) if flags.contains(OpenFlag::Create) => {
                let mut inode = INode::new([]);
                // TODO Apply umask
                inode.permissions = mode;
                let inode = Rc::new(RefCell::new(inode));
                state.file_system.save(&path, Rc::clone(&inode))?;
                inode
            }
            Err(errno) => return Err(errno),
        };

        let (is_readable, is_writable) = match access {
            OfdAccess::ReadOnly => (true, false),
            OfdAccess::WriteOnly => (false, true),
            OfdAccess::ReadWrite => (true, true),
            OfdAccess::Exec | OfdAccess::Search => (false, false),
        };

        if let FileBody::Fifo {
            readers, writers, ..
        } = &mut file.borrow_mut().body
        {
            if is_readable {
                *readers += 1;
            }
            if is_writable {
                *writers += 1;
            }
        }

        let open_file_description = Rc::new(RefCell::new(OpenFileDescription {
            file,
            offset: 0,
            is_readable,
            is_writable,
            is_appending: flags.contains(OpenFlag::Append),
        }));
        let body = FdBody {
            open_file_description,
            flag: if flags.contains(OpenFlag::Cloexec) {
                FdFlag::FD_CLOEXEC
            } else {
                FdFlag::empty()
            },
        };
        let process = state.processes.get_mut(&self.process_id).unwrap();
        process.open_fd(body).map_err(|_| Errno::EMFILE)
    }

    fn open_tmpfile(&mut self, _parent_dir: &Path) -> Result<Fd> {
        let file = Rc::new(RefCell::new(INode::new([])));
        let open_file_description = Rc::new(RefCell::new(OpenFileDescription {
            file,
            offset: 0,
            is_readable: true,
            is_writable: true,
            is_appending: false,
        }));
        let body = FdBody {
            open_file_description,
            flag: FdFlag::empty(),
        };
        let mut state = self.state.borrow_mut();
        let process = state.processes.get_mut(&self.process_id).unwrap();
        process.open_fd(body).map_err(|_| Errno::EMFILE)
    }

    fn close(&mut self, fd: Fd) -> Result<()> {
        self.current_process_mut().close_fd(fd);
        Ok(())
    }

    fn ofd_access(&self, fd: Fd) -> Result<OfdAccess> {
        fn is_directory(file_body: &FileBody) -> bool {
            matches!(file_body, FileBody::Directory { .. })
        }

        self.with_open_file_description(fd, |ofd| match (ofd.is_readable, ofd.is_writable) {
            (true, false) => Ok(OfdAccess::ReadOnly),
            (false, true) => Ok(OfdAccess::WriteOnly),
            (true, true) => Ok(OfdAccess::ReadWrite),
            (false, false) => {
                if is_directory(&ofd.i_node().borrow().body) {
                    Ok(OfdAccess::Search)
                } else {
                    Ok(OfdAccess::Exec)
                }
            }
        })
    }

    fn get_and_set_nonblocking(&mut self, fd: Fd, _nonblocking: bool) -> Result<bool> {
        self.with_open_file_description_mut(fd, |_ofd| {
            // TODO Implement non-blocking I/O
            Ok(false)
        })
    }

    fn fcntl_getfd(&self, fd: Fd) -> Result<FdFlag> {
        let process = self.current_process();
        let body = process.get_fd(fd).ok_or(Errno::EBADF)?;
        Ok(body.flag)
    }

    fn fcntl_setfd(&mut self, fd: Fd, flags: FdFlag) -> Result<()> {
        let mut process = self.current_process_mut();
        let body = process.get_fd_mut(fd).ok_or(Errno::EBADF)?;
        body.flag = flags;
        Ok(())
    }

    fn isatty(&self, _fd: Fd) -> Result<bool> {
        Ok(false)
    }

    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> Result<usize> {
        self.with_open_file_description_mut(fd, |ofd| ofd.read(buffer))
    }

    fn write(&mut self, fd: Fd, buffer: &[u8]) -> Result<usize> {
        self.with_open_file_description_mut(fd, |ofd| ofd.write(buffer))
    }

    fn lseek(&mut self, fd: Fd, position: SeekFrom) -> Result<u64> {
        use nix::unistd::Whence::*;
        let (offset, whence) = match position {
            SeekFrom::Start(offset) => {
                let offset = offset.try_into().map_err(|_| Errno::EOVERFLOW)?;
                (offset, SeekSet)
            }
            SeekFrom::End(offset) => {
                let offset = offset.try_into().map_err(|_| Errno::EOVERFLOW)?;
                (offset, SeekEnd)
            }
            SeekFrom::Current(offset) => {
                let offset = offset.try_into().map_err(|_| Errno::EOVERFLOW)?;
                (offset, SeekCur)
            }
        };
        self.with_open_file_description_mut(fd, |ofd| ofd.seek(offset, whence))
            .and_then(|new_offset| new_offset.try_into().map_err(|_| Errno::EOVERFLOW))
    }

    fn fdopendir(&mut self, fd: Fd) -> Result<Box<dyn Dir>> {
        self.with_open_file_description(fd, |ofd| {
            let inode = ofd.i_node();
            let dir = VirtualDir::try_from(&inode.borrow().body)?;
            Ok(Box::new(dir) as Box<dyn Dir>)
        })
    }

    fn opendir(&mut self, path: &CStr) -> Result<Box<dyn Dir>> {
        let fd = self.open(
            path,
            OfdAccess::ReadOnly,
            OpenFlag::Directory.into(),
            Mode::empty(),
        )?;
        self.fdopendir(fd)
    }

    fn umask(&mut self, new_mask: Mode) -> Mode {
        std::mem::replace(&mut self.current_process_mut().umask, new_mask)
    }

    /// Returns `now` in [`SystemState`].
    ///
    /// Panics if it is `None`.
    fn now(&self) -> Instant {
        self.state
            .borrow()
            .now
            .expect("SystemState::now not assigned")
    }

    /// Returns `times` in [`SystemState`].
    fn times(&self) -> Result<Times> {
        Ok(self.state.borrow().times)
    }

    fn validate_signal(&self, number: signal::RawNumber) -> Option<(signal::Name, signal::Number)> {
        let non_zero = NonZeroI32::new(number)?;
        let name = signal::Name::try_from_raw_virtual(number)?;
        Some((name, signal::Number::from_raw_unchecked(non_zero)))
    }

    #[inline(always)]
    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number> {
        name.to_raw_virtual()
    }

    fn sigmask(
        &mut self,
        op: Option<(SigmaskHow, &[signal::Number])>,
        old_mask: Option<&mut Vec<signal::Number>>,
    ) -> Result<()> {
        let mut state = self.state.borrow_mut();
        let process = state
            .processes
            .get_mut(&self.process_id)
            .expect("current process not found");

        if let Some(old_mask) = old_mask {
            old_mask.clear();
            old_mask.extend(process.blocked_signals());
        }

        if let Some((how, mask)) = op {
            let result = process.block_signals(how, mask);
            if result.process_state_changed {
                let parent_pid = process.ppid;
                raise_sigchld(&mut state, parent_pid);
            }
        }

        Ok(())
    }

    fn sigaction(
        &mut self,
        signal: signal::Number,
        action: SignalHandling,
    ) -> Result<SignalHandling> {
        let mut process = self.current_process_mut();
        Ok(process.set_signal_handling(signal, action))
    }

    fn caught_signals(&mut self) -> Vec<signal::Number> {
        std::mem::take(&mut self.current_process_mut().caught_signals)
    }

    /// Sends a signal to the target process.
    ///
    /// This function returns a future that enables the executor to block the
    /// calling thread until the current process is ready to proceed. If the
    /// signal is sent to the current process and it causes the process to stop,
    /// the future will be ready only when the process is resumed. Similarly, if
    /// the signal causes the current process to terminate, the future will
    /// never be ready.
    fn kill(
        &mut self,
        target: Pid,
        signal: Option<signal::Number>,
    ) -> Pin<Box<(dyn Future<Output = Result<()>>)>> {
        let result = match target {
            Pid::MY_PROCESS_GROUP => {
                let target_pgid = self.current_process().pgid;
                send_signal_to_processes(&mut self.state.borrow_mut(), Some(target_pgid), signal)
            }

            Pid::ALL => send_signal_to_processes(&mut self.state.borrow_mut(), None, signal),

            Pid(raw_pid) if raw_pid >= 0 => {
                let mut state = self.state.borrow_mut();
                match state.processes.get_mut(&target) {
                    Some(process) => {
                        if let Some(signal) = signal {
                            let result = process.raise_signal(signal);
                            if result.process_state_changed {
                                let parent_pid = process.ppid;
                                raise_sigchld(&mut state, parent_pid);
                            }
                        }
                        Ok(())
                    }
                    None => Err(Errno::ESRCH),
                }
            }

            Pid(negative_pgid) => {
                let target_pgid = Pid(-negative_pgid);
                send_signal_to_processes(&mut self.state.borrow_mut(), Some(target_pgid), signal)
            }
        };

        let system = self.clone();
        Box::pin(async move {
            system.block_until_running().await;
            result
        })
    }

    /// Waits for a next event.
    ///
    /// The `VirtualSystem` implementation for this method does not actually
    /// block the calling thread. The method returns immediately in any case.
    ///
    /// The `timeout` is ignored if this function returns because of a ready FD
    /// or a caught signal. Otherwise, the timeout is added to
    /// [`SystemState::now`], which must not be `None` then.
    fn select(
        &mut self,
        readers: &mut FdSet,
        writers: &mut FdSet,
        timeout: Option<&TimeSpec>,
        signal_mask: Option<&[signal::Number]>,
    ) -> Result<c_int> {
        let mut process = self.current_process_mut();

        if let Some(signal_mask) = signal_mask {
            let save_mask = process
                .blocked_signals()
                .iter()
                .copied()
                .collect::<Vec<signal::Number>>();
            let result_1 = process.block_signals(SigmaskHow::SIG_SETMASK, signal_mask);
            let result_2 = process.block_signals(SigmaskHow::SIG_SETMASK, &save_mask);
            assert!(!result_2.delivered);
            if result_1.caught {
                return Err(Errno::EINTR);
            }
        }

        for fd in &readers.clone() {
            let body = process.fds().get(&fd).ok_or(Errno::EBADF)?;
            let ofd = body.open_file_description.borrow();
            if !ofd.is_readable() {
                return Err(Errno::EBADF);
            }
            if !ofd.is_ready_for_reading() {
                readers.remove(fd);
            }
        }
        for fd in &writers.clone() {
            let body = process.fds().get(&fd).ok_or(Errno::EBADF)?;
            let ofd = body.open_file_description.borrow();
            if !ofd.is_writable() {
                return Err(Errno::EBADF);
            }
            if !ofd.is_ready_for_writing() {
                writers.remove(fd);
            }
        }

        drop(process);

        let reader_count = readers.iter().count();
        let writer_count = writers.iter().count();
        let count = (reader_count + writer_count).try_into().unwrap();
        if count == 0 {
            if let Some(timeout) = timeout {
                let duration = Duration::from(*timeout);
                if !duration.is_zero() {
                    let mut state = self.state.borrow_mut();
                    let now = state.now.as_mut();
                    let now = now.expect("now time unspecified; cannot add timeout duration");
                    *now += duration;
                }
            }
        }
        Ok(count)
    }

    fn getpid(&self) -> Pid {
        self.process_id
    }

    fn getppid(&self) -> Pid {
        self.current_process().ppid
    }

    fn getpgrp(&self) -> Pid {
        self.current_process().pgid
    }

    /// Modifies the process group ID of a process.
    ///
    /// The current implementation does not yet support the concept of sessions.
    fn setpgid(&mut self, mut pid: Pid, mut pgid: Pid) -> Result<()> {
        if pgid.0 < 0 {
            return Err(Errno::EINVAL);
        }
        if pid.0 == 0 {
            pid = self.process_id;
        }
        if pgid.0 == 0 {
            pgid = pid;
        }

        let mut state = self.state.borrow_mut();
        if pgid != pid && !state.processes.values().any(|p| p.pgid == pgid) {
            return Err(Errno::EPERM);
        }
        let process = state.processes.get_mut(&pid).ok_or(Errno::ESRCH)?;
        if pid != self.process_id && process.ppid != self.process_id {
            return Err(Errno::ESRCH);
        }
        if process.last_exec.is_some() {
            return Err(Errno::EACCES);
        }

        process.pgid = pgid;
        Ok(())
        // TODO Support sessions
    }

    /// Returns the current foreground process group ID.
    ///
    /// The current implementation does not yet support the concept of
    /// controlling terminals and sessions. It accepts any open file descriptor.
    fn tcgetpgrp(&self, fd: Fd) -> Result<Pid> {
        // Make sure the FD is open
        self.with_open_file_description(fd, |_| Ok(()))?;

        self.state.borrow().foreground.ok_or(Errno::ENOTTY)
    }

    /// Switches the foreground process.
    ///
    /// The current implementation does not yet support the concept of
    /// controlling terminals and sessions. It accepts any open file descriptor.
    fn tcsetpgrp(&mut self, fd: Fd, pgid: Pid) -> Result<()> {
        // Make sure the FD is open
        self.with_open_file_description(fd, |_| Ok(()))?;

        // Make sure the process group exists
        let mut state = self.state.borrow_mut();
        if !state.processes.values().any(|p| p.pgid == pgid) {
            return Err(Errno::EPERM);
        }

        state.foreground = Some(pgid);
        Ok(())
    }

    /// Creates a new child process.
    ///
    /// This implementation does not create any real child process. Instead,
    /// it returns a child process starter that runs its task concurrently in
    /// the same process.
    ///
    /// To run the concurrent task, this function needs an executor that has
    /// been set in the system state. If the system state does not have an
    /// executor, this function fails with `Errno::ENOSYS`.
    ///
    /// The process ID of the child will be the maximum of existing process IDs
    /// plus 1. If there are no other processes, it will be 2.
    fn new_child_process(&mut self) -> Result<ChildProcessStarter> {
        let mut state = self.state.borrow_mut();
        let executor = state.executor.clone().ok_or(Errno::ENOSYS)?;
        let process_id = state
            .processes
            .keys()
            .max()
            .map_or(Pid(2), |pid| Pid(pid.0 + 1));
        let parent_process = &state.processes[&self.process_id];
        let child_process = Process::fork_from(self.process_id, parent_process);
        state.processes.insert(process_id, child_process);
        drop(state);

        let state = Rc::clone(&self.state);
        Ok(Box::new(move |parent_env, task| {
            Box::pin(async move {
                let mut system = VirtualSystem { state, process_id };
                let mut child_env = parent_env.clone_with_system(Box::new(system.clone()));

                {
                    let mut process = system.current_process_mut();
                    process.selector = Rc::downgrade(&child_env.system.0);
                }

                let run_task_and_set_exit_status = Box::pin(async move {
                    let mut runner = ProcessRunner {
                        task: task(&mut child_env),
                        system,
                        waker: Rc::new(Cell::new(None)),
                    };
                    (&mut runner).await;

                    let ProcessRunner { system, .. } = { runner };
                    let mut state = system.state.borrow_mut();
                    let process = state
                        .processes
                        .get_mut(&process_id)
                        .expect("missing child process");
                    if process.state == ProcessState::Running
                        && process.set_state(ProcessState::exited(child_env.exit_status))
                    {
                        let ppid = process.ppid;
                        raise_sigchld(&mut state, ppid);
                    }
                });

                executor
                    .spawn(run_task_and_set_exit_status)
                    .expect("the executor failed to start the child process task");

                process_id
            })
        }))
    }

    /// Waits for a child.
    ///
    /// TODO: Currently, this function only supports `target == -1 || target > 0`.
    fn wait(&mut self, target: Pid) -> Result<Option<(Pid, ProcessState)>> {
        let parent_pid = self.process_id;
        let mut state = self.state.borrow_mut();
        if let Some((pid, process)) = state.child_to_wait_for(parent_pid, target) {
            if process.state_has_changed() {
                Ok(Some((pid, process.take_state())))
            } else if process.state().is_alive() {
                Ok(None)
            } else {
                Err(Errno::ECHILD)
            }
        } else {
            Err(Errno::ECHILD)
        }
    }

    /// Stub for the `execve` system call.
    ///
    /// The `execve` system call cannot be simulated in the userland. This
    /// function returns `ENOSYS` if the file at `path` is a native executable,
    /// `ENOEXEC` if a non-executable file, and `ENOENT` otherwise.
    fn execve(&mut self, path: &CStr, args: &[CString], envs: &[CString]) -> Result<Infallible> {
        let os_path = OsStr::from_bytes(path.to_bytes());
        let mut state = self.state.borrow_mut();
        let fs = &state.file_system;
        let file = fs.get(os_path)?;
        // TODO Check file permissions
        let is_executable = matches!(
            &file.borrow().body,
            FileBody::Regular {
                is_native_executable: true,
                ..
            }
        );
        if is_executable {
            // Save arguments in the Process
            let process = state.processes.get_mut(&self.process_id).unwrap();
            let path = path.to_owned();
            let args = args.to_owned();
            let envs = envs.to_owned();
            process.last_exec = Some((path, args, envs));

            Err(Errno::ENOSYS)
        } else {
            Err(Errno::ENOEXEC)
        }
    }

    fn getcwd(&self) -> Result<PathBuf> {
        Ok(self.current_process().cwd.clone())
    }

    /// Changes the current working directory.
    ///
    /// The current implementation does not canonicalize ".", "..", or symbolic
    /// links in the new path set to the process.
    fn chdir(&mut self, path: &CStr) -> Result<()> {
        let path = Path::new(OsStr::from_bytes(path.to_bytes()));
        let inode = self.resolve_existing_file(AT_FDCWD, path, AtFlags::empty())?;
        if matches!(&inode.borrow().body, FileBody::Directory { .. }) {
            let mut process = self.current_process_mut();
            let new_path = process.cwd.join(path);
            process.chdir(new_path);
            Ok(())
        } else {
            Err(Errno::ENOTDIR)
        }
    }

    fn getuid(&self) -> Uid {
        self.current_process().uid()
    }

    fn geteuid(&self) -> Uid {
        self.current_process().euid()
    }

    fn getgid(&self) -> Gid {
        self.current_process().gid()
    }

    fn getegid(&self) -> Gid {
        self.current_process().egid()
    }

    fn getpwnam_dir(&self, name: &str) -> Result<Option<PathBuf>> {
        let state = self.state.borrow();
        Ok(state.home_dirs.get(name).cloned())
    }

    /// Returns the standard path for the system.
    ///
    /// This function returns the value of [`SystemState::path`]. If it is empty,
    /// it returns the `ENOSYS` error.
    fn confstr_path(&self) -> Result<OsString> {
        let path = self.state.borrow().path.clone();
        if path.is_empty() {
            Err(Errno::ENOSYS)
        } else {
            Ok(path)
        }
    }

    /// Returns the path to the shell.
    ///
    /// The current implementation returns "/bin/sh".
    fn shell_path(&self) -> CString {
        c"/bin/sh".to_owned()
    }

    fn getrlimit(&self, resource: Resource) -> std::io::Result<LimitPair> {
        let process = self.current_process();
        Ok(process
            .resource_limits
            .get(&resource)
            .copied()
            .unwrap_or(LimitPair {
                soft: RLIM_INFINITY,
                hard: RLIM_INFINITY,
            }))
    }

    fn setrlimit(&mut self, resource: Resource, limits: LimitPair) -> std::io::Result<()> {
        if limits.soft_exceeds_hard() {
            return Err(std::io::Error::from_raw_os_error(nix::libc::EINVAL));
        }

        let mut process = self.current_process_mut();
        use std::collections::hash_map::Entry::{Occupied, Vacant};
        match process.resource_limits.entry(resource) {
            Occupied(occupied) => {
                let occupied = occupied.into_mut();
                if limits.hard > occupied.hard {
                    return Err(std::io::Error::from_raw_os_error(nix::libc::EPERM));
                }
                *occupied = limits;
            }
            Vacant(vacant) => {
                vacant.insert(limits);
            }
        }
        Ok(())
    }
}

fn send_signal_to_processes(
    state: &mut SystemState,
    target_pgid: Option<Pid>,
    signal: Option<signal::Number>,
) -> Result<()> {
    let mut results = Vec::new();

    if let Some(signal) = signal {
        for (&_pid, process) in &mut state.processes {
            if target_pgid.map_or(true, |target_pgid| process.pgid == target_pgid) {
                let result = process.raise_signal(signal);
                results.push((result, process.ppid));
            }
        }
    }

    if results.is_empty() {
        Err(Errno::ESRCH)
    } else {
        for (result, ppid) in results {
            if result.process_state_changed {
                raise_sigchld(state, ppid);
            }
        }
        Ok(())
    }
}

fn raise_sigchld(state: &mut SystemState, target_pid: Pid) {
    if let Some(target) = state.processes.get_mut(&target_pid) {
        let result = target.raise_signal(signal::SIGCHLD);
        assert!(!result.process_state_changed);
    }
}

/// State of the virtual system.
#[derive(Clone, Debug, Default)]
pub struct SystemState {
    /// Current time
    pub now: Option<Instant>,

    /// Consumed CPU time
    pub times: Times,

    /// Task manager that can execute asynchronous tasks
    ///
    /// The virtual system uses this executor to run (virtual) child processes.
    /// If `executor` is `None`, [`VirtualSystem::new_child_process`] will fail.
    pub executor: Option<Rc<dyn Executor>>,

    /// Processes running in the system
    pub processes: BTreeMap<Pid, Process>,

    /// Process group ID of the foreground process group
    ///
    /// Note: The current implementation does not support the notion of
    /// controlling terminals and sessions. This item may be replaced with a
    /// more _correct_ implementation in the future.
    pub foreground: Option<Pid>,

    /// Collection of files existing in the virtual system
    pub file_system: FileSystem,

    /// Map from user names to their home directory paths
    ///
    /// [`VirtualSystem::getpwnam_dir`] looks up its argument in this
    /// dictionary.
    pub home_dirs: HashMap<String, PathBuf>,

    /// Standard path returned by [`VirtualSystem::confstr_path`]
    pub path: OsString,
}

impl SystemState {
    /// Performs [`select`](crate::system::SharedSystem::select) on all
    /// processes in the system.
    ///
    /// Any errors are ignored.
    ///
    /// The `RefCell` must not have been borrowed, or this function will panic
    /// with a double borrow.
    pub fn select_all(this: &RefCell<Self>) {
        let mut selectors = Vec::new();
        for process in this.borrow().processes.values() {
            if let Some(selector) = process.selector.upgrade() {
                selectors.push(selector);
            }
        }
        // To avoid double borrowing, SelectSystem::select must be called after
        // dropping the borrow for `this`
        for selector in selectors {
            // TODO merge advances of `now` performed by each select
            selector.borrow_mut().select(false).ok();
        }
    }

    /// Finds a child process to wait for.
    ///
    /// This is a helper function for `VirtualSystem::wait`.
    fn child_to_wait_for(&mut self, parent_pid: Pid, target: Pid) -> Option<(Pid, &mut Process)> {
        match target.0 {
            0 => todo!("wait target {}", target),
            -1 => {
                // any child
                let mut result = None;
                for (pid, process) in &mut self.processes {
                    if process.ppid == parent_pid {
                        let changed = process.state_has_changed();
                        result = Some((*pid, process));
                        if changed {
                            break;
                        }
                    }
                }
                result
            }
            raw if raw >= 0 => {
                let process = self.processes.get_mut(&target)?;
                if process.ppid == parent_pid {
                    Some((target, process))
                } else {
                    None
                }
            }
            _target => todo!("wait target {}", target),
        }
    }
}

/// Executor that can start new async tasks.
///
/// This trait abstracts the executor interface so that [`SystemState`] does not
/// depend on a specific executor implementation.
///
/// Note that [`VirtualSystem`] does not support multi-threading. The executor
/// should run concurrent tasks on a single thread.
pub trait Executor: Debug {
    /// Starts a new async task.
    ///
    /// Returns `Ok(())` if the task has been started successfully and `Err(_)`
    /// otherwise.
    fn spawn(
        &self,
        task: Pin<Box<dyn Future<Output = ()>>>,
    ) -> std::result::Result<(), Box<dyn std::error::Error>>;
}

/// Concurrent task that manages the execution of a process.
///
/// This struct is a helper for [`VirtualSystem::new_child_process`].
/// It basically runs the given task, but pauses or cancels it depending on
/// the state of the process.
struct ProcessRunner<'a> {
    task: Pin<Box<dyn Future<Output = ()> + 'a>>,
    system: VirtualSystem,

    /// Waker that is woken up when the process is resumed.
    waker: Rc<Cell<Option<Waker>>>,
}

impl Future for ProcessRunner<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.deref_mut();

        let process_state = this.system.current_process().state;
        if process_state == ProcessState::Running {
            // Let the task make progress
            let poll = this.task.as_mut().poll(cx);
            if poll == Poll::Ready(()) {
                return Poll::Ready(());
            }
        }

        let mut process = this.system.current_process_mut();
        match process.state {
            ProcessState::Running => Poll::Pending,
            ProcessState::Halted(result) => {
                if result.is_stopped() {
                    this.waker.set(Some(cx.waker().clone()));
                    process.wake_on_resumption(Rc::downgrade(&this.waker));
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::ProcessResult;
    use crate::semantics::ExitStatus;
    use crate::Env;
    use assert_matches::assert_matches;
    use futures_executor::LocalPool;
    use futures_util::FutureExt;
    use std::ffi::CString;
    use std::ffi::OsString;
    use std::future::pending;

    impl Executor for futures_executor::LocalSpawner {
        fn spawn(
            &self,
            task: Pin<Box<dyn Future<Output = ()>>>,
        ) -> std::result::Result<(), Box<dyn std::error::Error>> {
            use futures_util::task::LocalSpawnExt;
            self.spawn_local(task)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        }
    }

    #[test]
    fn fstatat_non_existent_file() {
        let system = VirtualSystem::new();
        assert_matches!(
            system.fstatat(Fd(0), c"/no/such/file", AtFlags::empty()),
            Err(Errno::ENOENT)
        );
    }

    #[test]
    fn fstatat_regular_file() {
        let system = VirtualSystem::new();
        let path = "/some/file";
        let content = Rc::new(RefCell::new(INode::new([1, 2, 3, 42, 100])));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);

        let stat = system
            .fstatat(Fd(0), c"/some/file", AtFlags::empty())
            .unwrap();
        assert_eq!(stat.st_mode, SFlag::S_IFREG.bits() | Mode::default().bits());
        assert_eq!(stat.st_size, 5);
        // TODO Other stat properties
    }

    #[test]
    fn fstatat_directory() {
        let system = VirtualSystem::new();
        let path = "/some/file";
        let content = Rc::new(RefCell::new(INode::new([])));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);

        let stat = system.fstatat(Fd(0), c"/some/", AtFlags::empty()).unwrap();
        assert_eq!(stat.st_mode, SFlag::S_IFDIR.bits() | 0o755);
        // TODO Other stat properties
    }

    #[test]
    fn fstatat_fifo() {
        let system = VirtualSystem::new();
        let path = "/some/fifo";
        let content = Rc::new(RefCell::new(INode {
            body: FileBody::Fifo {
                content: [17; 42].into(),
                readers: 0,
                writers: 0,
            },
            permissions: Mode::default(),
        }));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);

        let stat = system
            .fstatat(Fd(0), c"/some/fifo", AtFlags::empty())
            .unwrap();
        assert_eq!(stat.st_mode, SFlag::S_IFIFO.bits() | Mode::default().bits());
        assert_eq!(stat.st_size, 42);
    }

    fn system_with_symlink() -> VirtualSystem {
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();
        state
            .file_system
            .save("/some/file", Rc::new(RefCell::new(INode::new([]))))
            .unwrap();
        state
            .file_system
            .save(
                "/link",
                Rc::new(RefCell::new(INode {
                    body: FileBody::Symlink {
                        target: "some/file".into(),
                    },
                    permissions: Mode::default(),
                })),
            )
            .unwrap();
        drop(state);
        system
    }

    #[test]
    fn fstatat_symlink_to_regular_file() {
        let system = system_with_symlink();
        let stat = system.fstatat(Fd(0), c"/link", AtFlags::empty()).unwrap();
        assert_eq!(stat.st_mode, SFlag::S_IFREG.bits() | Mode::default().bits());
    }

    #[test]
    fn fstatat_symlink_no_follow() {
        let system = system_with_symlink();
        let stat = system
            .fstatat(Fd(0), c"/link", AtFlags::AT_SYMLINK_NOFOLLOW)
            .unwrap();
        assert_eq!(stat.st_mode, SFlag::S_IFLNK.bits() | Mode::default().bits());
    }

    #[test]
    fn is_executable_file_non_existing_file() {
        let system = VirtualSystem::new();
        assert!(!system.is_executable_file(c"/no/such/file"));
    }

    #[test]
    fn is_executable_file_existing_but_non_executable_file() {
        let system = VirtualSystem::new();
        let path = "/some/file";
        let content = Rc::new(RefCell::new(INode::default()));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);
        assert!(!system.is_executable_file(c"/some/file"));
    }

    #[test]
    fn is_executable_file_with_executable_file() {
        let system = VirtualSystem::new();
        let path = "/some/file";
        let mut content = INode::default();
        content.permissions.set(Mode::USER_EXEC, true);
        let content = Rc::new(RefCell::new(content));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);
        assert!(system.is_executable_file(c"/some/file"));
    }

    #[test]
    fn pipe_read_write() {
        let mut system = VirtualSystem::new();
        let (reader, writer) = system.pipe().unwrap();
        let result = system.write(writer, &[5, 42, 29]);
        assert_eq!(result, Ok(3));

        let mut buffer = [1; 4];
        let result = system.read(reader, &mut buffer);
        assert_eq!(result, Ok(3));
        assert_eq!(buffer, [5, 42, 29, 1]);

        let result = system.close(writer);
        assert_eq!(result, Ok(()));

        let result = system.read(reader, &mut buffer);
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn dup_shares_open_file_description() {
        let mut system = VirtualSystem::new();
        let result = system.dup(Fd::STDOUT, Fd::STDERR, FdFlag::empty());
        assert_eq!(result, Ok(Fd(3)));

        let process = system.current_process();
        let fd1 = process.fds.get(&Fd(1)).unwrap();
        let fd3 = process.fds.get(&Fd(3)).unwrap();
        assert_eq!(fd1, fd3);
    }

    #[test]
    fn dup_can_set_cloexec() {
        let mut system = VirtualSystem::new();
        let result = system.dup(Fd::STDOUT, Fd::STDERR, FdFlag::FD_CLOEXEC);
        assert_eq!(result, Ok(Fd(3)));

        let process = system.current_process();
        let fd3 = process.fds.get(&Fd(3)).unwrap();
        assert_eq!(fd3.flag, FdFlag::FD_CLOEXEC);
    }

    #[test]
    fn dup2_shares_open_file_description() {
        let mut system = VirtualSystem::new();
        let result = system.dup2(Fd::STDOUT, Fd(5));
        assert_eq!(result, Ok(Fd(5)));

        let process = system.current_process();
        let fd1 = process.fds.get(&Fd(1)).unwrap();
        let fd5 = process.fds.get(&Fd(5)).unwrap();
        assert_eq!(fd1, fd5);
    }

    #[test]
    fn dup2_clears_cloexec() {
        let mut system = VirtualSystem::new();
        let mut process = system.current_process_mut();
        process.fds.get_mut(&Fd::STDOUT).unwrap().flag = FdFlag::FD_CLOEXEC;
        drop(process);

        let result = system.dup2(Fd::STDOUT, Fd(6));
        assert_eq!(result, Ok(Fd(6)));

        let process = system.current_process();
        let fd6 = process.fds.get(&Fd(6)).unwrap();
        assert_eq!(fd6.flag, FdFlag::empty());
    }

    #[test]
    fn open_non_existing_file_no_creation() {
        let mut system = VirtualSystem::new();
        let result = system.open(
            c"/no/such/file",
            OfdAccess::ReadOnly,
            EnumSet::empty(),
            Mode::empty(),
        );
        assert_eq!(result, Err(Errno::ENOENT));
    }

    #[test]
    fn open_creating_non_existing_file() {
        let mut system = VirtualSystem::new();
        let result = system.open(
            c"new_file",
            OfdAccess::WriteOnly,
            OpenFlag::Create.into(),
            Mode::empty(),
        );
        assert_eq!(result, Ok(Fd(3)));

        system.write(Fd(3), &[42, 123]).unwrap();
        let file = system.state.borrow().file_system.get("new_file").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], [42, 123]);
        });
    }

    #[test]
    fn open_existing_file() {
        let mut system = VirtualSystem::new();
        let fd = system
            .open(
                c"file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .unwrap();
        system.write(fd, &[75, 96, 133]).unwrap();

        let result = system.open(
            c"file",
            OfdAccess::ReadOnly,
            EnumSet::empty(),
            Mode::empty(),
        );
        assert_eq!(result, Ok(Fd(4)));

        let mut buffer = [0; 5];
        let count = system.read(Fd(4), &mut buffer).unwrap();
        assert_eq!(count, 3);
        assert_eq!(buffer, [75, 96, 133, 0, 0]);
        let count = system.read(Fd(4), &mut buffer).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn open_existing_file_excl() {
        let mut system = VirtualSystem::new();
        let first = system.open(
            c"my_file",
            OfdAccess::WriteOnly,
            OpenFlag::Create | OpenFlag::Exclusive,
            Mode::empty(),
        );
        assert_eq!(first, Ok(Fd(3)));

        let second = system.open(
            c"my_file",
            OfdAccess::WriteOnly,
            OpenFlag::Create | OpenFlag::Exclusive,
            Mode::empty(),
        );
        assert_eq!(second, Err(Errno::EEXIST));
    }

    #[test]
    fn open_truncating() {
        let mut system = VirtualSystem::new();
        let fd = system
            .open(
                c"file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::ALL_9,
            )
            .unwrap();
        system.write(fd, &[1, 2, 3]).unwrap();

        let result = system.open(
            c"file",
            OfdAccess::WriteOnly,
            OpenFlag::Truncate.into(),
            Mode::empty(),
        );
        assert_eq!(result, Ok(Fd(4)));

        let reader = system
            .open(
                c"file",
                OfdAccess::ReadOnly,
                EnumSet::empty(),
                Mode::empty(),
            )
            .unwrap();
        let count = system.read(reader, &mut [0; 1]).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn open_appending() {
        let mut system = VirtualSystem::new();
        let fd = system
            .open(
                c"file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::ALL_9,
            )
            .unwrap();
        system.write(fd, &[1, 2, 3]).unwrap();

        let result = system.open(
            c"file",
            OfdAccess::WriteOnly,
            OpenFlag::Append.into(),
            Mode::empty(),
        );
        assert_eq!(result, Ok(Fd(4)));
        system.write(Fd(4), &[4, 5, 6]).unwrap();

        let reader = system
            .open(
                c"file",
                OfdAccess::ReadOnly,
                EnumSet::empty(),
                Mode::empty(),
            )
            .unwrap();
        let mut buffer = [0; 7];
        let count = system.read(reader, &mut buffer).unwrap();
        assert_eq!(count, 6);
        assert_eq!(buffer, [1, 2, 3, 4, 5, 6, 0]);
    }

    #[test]
    fn open_directory() {
        let mut system = VirtualSystem::new();

        // Create a regular file and its parent directory
        let _ = system.open(
            c"/dir/file",
            OfdAccess::WriteOnly,
            OpenFlag::Create.into(),
            Mode::empty(),
        );

        let result = system.open(
            c"/dir",
            OfdAccess::ReadOnly,
            OpenFlag::Directory.into(),
            Mode::empty(),
        );
        assert_eq!(result, Ok(Fd(4)));
    }

    #[test]
    fn open_non_directory_path_prefix() {
        let mut system = VirtualSystem::new();

        // Create a regular file
        let _ = system.open(
            c"/file",
            OfdAccess::WriteOnly,
            OpenFlag::Create.into(),
            Mode::empty(),
        );

        let result = system.open(
            c"/file/file",
            OfdAccess::WriteOnly,
            OpenFlag::Create.into(),
            Mode::empty(),
        );
        assert_eq!(result, Err(Errno::ENOTDIR));
    }

    #[test]
    fn open_non_directory_file() {
        let mut system = VirtualSystem::new();

        // Create a regular file
        let _ = system.open(
            c"/file",
            OfdAccess::WriteOnly,
            OpenFlag::Create.into(),
            Mode::empty(),
        );

        let result = system.open(
            c"/file",
            OfdAccess::ReadOnly,
            OpenFlag::Directory.into(),
            Mode::empty(),
        );
        assert_eq!(result, Err(Errno::ENOTDIR));
    }

    #[test]
    fn open_default_working_directory() {
        // The default working directory is the root directory.
        let mut system = VirtualSystem::new();

        let writer = system.open(
            c"/dir/file",
            OfdAccess::WriteOnly,
            OpenFlag::Create.into(),
            Mode::ALL_9,
        );
        system.write(writer.unwrap(), &[1, 2, 3, 42]).unwrap();

        let reader = system.open(
            c"./dir/file",
            OfdAccess::ReadOnly,
            EnumSet::empty(),
            Mode::empty(),
        );
        let mut buffer = [0; 10];
        let count = system.read(reader.unwrap(), &mut buffer).unwrap();
        assert_eq!(count, 4);
        assert_eq!(buffer[0..4], [1, 2, 3, 42]);
    }

    #[test]
    fn open_tmpfile() {
        let mut system = VirtualSystem::new();
        let fd = system.open_tmpfile(Path::new("")).unwrap();
        system.write(fd, &[42, 17, 75]).unwrap();
        system.lseek(fd, SeekFrom::Start(0)).unwrap();
        let mut buffer = [0; 4];
        let count = system.read(fd, &mut buffer).unwrap();
        assert_eq!(count, 3);
        assert_eq!(buffer[..3], [42, 17, 75]);
    }

    #[test]
    fn close() {
        let mut system = VirtualSystem::new();

        let result = system.close(Fd::STDERR);
        assert_eq!(result, Ok(()));
        assert_eq!(system.current_process().fds.get(&Fd::STDERR), None);

        let result = system.close(Fd::STDERR);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn fcntl_getfd_and_setfd() {
        let mut system = VirtualSystem::new();

        let flags = system.fcntl_getfd(Fd::STDIN).unwrap();
        assert_eq!(flags, FdFlag::empty());

        system.fcntl_setfd(Fd::STDIN, FdFlag::FD_CLOEXEC).unwrap();

        let flags = system.fcntl_getfd(Fd::STDIN).unwrap();
        assert_eq!(flags, FdFlag::FD_CLOEXEC);

        let flags = system.fcntl_getfd(Fd::STDOUT).unwrap();
        assert_eq!(flags, FdFlag::empty());

        system.fcntl_setfd(Fd::STDIN, FdFlag::empty()).unwrap();

        let flags = system.fcntl_getfd(Fd::STDIN).unwrap();
        assert_eq!(flags, FdFlag::empty());
    }

    #[test]
    fn opendir_default_working_directory() {
        // The default working directory is the root directory.
        let mut system = VirtualSystem::new();

        let _ = system.open(
            c"/dir/file",
            OfdAccess::WriteOnly,
            OpenFlag::Create.into(),
            Mode::ALL_9,
        );

        let mut dir = system.opendir(c"./dir").unwrap();
        let mut files = Vec::new();
        while let Some(entry) = dir.next().unwrap() {
            files.push(entry.name.to_os_string());
        }
        files.sort_unstable();
        assert_eq!(
            files[..],
            [
                OsString::from("."),
                OsString::from(".."),
                OsString::from("file")
            ]
        );
    }

    // TODO Test sigmask

    #[test]
    fn kill_process() {
        let mut system = VirtualSystem::new();
        system
            .kill(system.process_id, None)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(system.current_process().state(), ProcessState::Running);

        let result = system.kill(system.process_id, Some(SIGINT)).now_or_never();
        // The future should be pending because the current process has been killed
        assert_eq!(result, None);
        assert_eq!(
            system.current_process().state(),
            ProcessState::Halted(ProcessResult::Signaled {
                signal: SIGINT,
                core_dump: false
            })
        );

        let mut system = VirtualSystem::new();
        let state = system.state.borrow();
        let max_pid = *state.processes.keys().max().unwrap();
        drop(state);
        let e = system
            .kill(Pid(max_pid.0 + 1), Some(SIGINT))
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(e, Errno::ESRCH);
    }

    #[test]
    fn kill_all_processes() {
        let mut system = VirtualSystem::new();
        let pgid = system.current_process().pgid;
        let mut state = system.state.borrow_mut();
        state.processes.insert(
            Pid(10),
            Process::with_parent_and_group(system.process_id, pgid),
        );
        state.processes.insert(
            Pid(11),
            Process::with_parent_and_group(system.process_id, pgid),
        );
        state
            .processes
            .insert(Pid(21), Process::with_parent_and_group(Pid(10), Pid(21)));
        drop(state);

        let result = system.kill(Pid::ALL, Some(SIGTERM)).now_or_never();
        // The future should be pending because the current process has been killed
        assert_eq!(result, None);
        let state = system.state.borrow();
        for process in state.processes.values() {
            assert_eq!(
                process.state,
                ProcessState::Halted(ProcessResult::Signaled {
                    signal: SIGTERM,
                    core_dump: false
                })
            );
        }
    }

    #[test]
    fn kill_processes_in_same_group() {
        let mut system = VirtualSystem::new();
        let pgid = system.current_process().pgid;
        let mut state = system.state.borrow_mut();
        state.processes.insert(
            Pid(10),
            Process::with_parent_and_group(system.process_id, pgid),
        );
        state.processes.insert(
            Pid(11),
            Process::with_parent_and_group(system.process_id, pgid),
        );
        state
            .processes
            .insert(Pid(21), Process::with_parent_and_group(Pid(10), Pid(21)));
        drop(state);

        let result = system
            .kill(Pid::MY_PROCESS_GROUP, Some(SIGQUIT))
            .now_or_never();
        // The future should be pending because the current process has been killed
        assert_eq!(result, None);
        let state = system.state.borrow();
        assert_eq!(
            state.processes[&system.process_id].state,
            ProcessState::Halted(ProcessResult::Signaled {
                signal: SIGQUIT,
                core_dump: true
            })
        );
        assert_eq!(
            state.processes[&Pid(10)].state,
            ProcessState::Halted(ProcessResult::Signaled {
                signal: SIGQUIT,
                core_dump: true
            })
        );
        assert_eq!(
            state.processes[&Pid(11)].state,
            ProcessState::Halted(ProcessResult::Signaled {
                signal: SIGQUIT,
                core_dump: true
            })
        );
        assert_eq!(state.processes[&Pid(21)].state, ProcessState::Running);
    }

    #[test]
    fn kill_process_group() {
        let mut system = VirtualSystem::new();
        let pgid = system.current_process().pgid;
        let mut state = system.state.borrow_mut();
        state.processes.insert(
            Pid(10),
            Process::with_parent_and_group(system.process_id, pgid),
        );
        state.processes.insert(
            Pid(11),
            Process::with_parent_and_group(system.process_id, Pid(21)),
        );
        state
            .processes
            .insert(Pid(21), Process::with_parent_and_group(Pid(10), Pid(21)));
        drop(state);

        system
            .kill(Pid(-21), Some(SIGHUP))
            .now_or_never()
            .unwrap()
            .unwrap();
        let state = system.state.borrow();
        assert_eq!(
            state.processes[&system.process_id].state,
            ProcessState::Running
        );
        assert_eq!(state.processes[&Pid(10)].state, ProcessState::Running);
        assert_eq!(
            state.processes[&Pid(11)].state,
            ProcessState::Halted(ProcessResult::Signaled {
                signal: SIGHUP,
                core_dump: false
            })
        );
        assert_eq!(
            state.processes[&Pid(21)].state,
            ProcessState::Halted(ProcessResult::Signaled {
                signal: SIGHUP,
                core_dump: false
            })
        );
    }

    #[test]
    fn kill_returns_success_even_if_process_state_did_not_change() {
        let mut system = VirtualSystem::new();
        let pgid = system.current_process().pgid;
        let mut state = system.state.borrow_mut();
        state.processes.insert(
            Pid(10),
            Process::with_parent_and_group(system.process_id, pgid),
        );
        drop(state);

        system
            .kill(-pgid, Some(SIGCONT))
            .now_or_never()
            .unwrap()
            .unwrap();
        let state = system.state.borrow();
        assert_eq!(state.processes[&Pid(10)].state, ProcessState::Running);
    }

    #[test]
    fn select_regular_file_is_always_ready() {
        let mut system = VirtualSystem::new();
        let mut readers = FdSet::new();
        readers.insert(Fd::STDIN).unwrap();
        let mut writers = FdSet::new();
        readers.insert(Fd::STDOUT).unwrap();
        readers.insert(Fd::STDERR).unwrap();

        let all_readers = readers;
        let all_writers = writers;
        let result = system.select(&mut readers, &mut writers, None, None);
        assert_eq!(result, Ok(3));
        assert_eq!(readers, all_readers);
        assert_eq!(writers, all_writers);
    }

    #[test]
    fn select_pipe_reader_is_ready_if_writer_is_closed() {
        let mut system = VirtualSystem::new();
        let (reader, writer) = system.pipe().unwrap();
        system.close(writer).unwrap();
        let mut readers = FdSet::new();
        let mut writers = FdSet::new();
        readers.insert(reader).unwrap();

        let all_readers = readers;
        let all_writers = writers;
        let result = system.select(&mut readers, &mut writers, None, None);
        assert_eq!(result, Ok(1));
        assert_eq!(readers, all_readers);
        assert_eq!(writers, all_writers);
    }

    #[test]
    fn select_pipe_reader_is_ready_if_something_has_been_written() {
        let mut system = VirtualSystem::new();
        let (reader, writer) = system.pipe().unwrap();
        system.write(writer, &[0]).unwrap();
        let mut readers = FdSet::new();
        let mut writers = FdSet::new();
        readers.insert(reader).unwrap();

        let all_readers = readers;
        let all_writers = writers;
        let result = system.select(&mut readers, &mut writers, None, None);
        assert_eq!(result, Ok(1));
        assert_eq!(readers, all_readers);
        assert_eq!(writers, all_writers);
    }

    #[test]
    fn select_pipe_reader_is_not_ready_if_writer_has_written_nothing() {
        let mut system = VirtualSystem::new();
        let (reader, _writer) = system.pipe().unwrap();
        let mut readers = FdSet::new();
        let mut writers = FdSet::new();
        readers.insert(reader).unwrap();

        let result = system.select(&mut readers, &mut writers, None, None);
        assert_eq!(result, Ok(0));
        assert_eq!(readers, FdSet::new());
        assert_eq!(writers, FdSet::new());
    }

    #[test]
    fn select_pipe_writer_is_ready_if_pipe_is_not_full() {
        let mut system = VirtualSystem::new();
        let (_reader, writer) = system.pipe().unwrap();
        let mut readers = FdSet::new();
        let mut writers = FdSet::new();
        writers.insert(writer).unwrap();

        let all_readers = readers;
        let all_writers = writers;
        let result = system.select(&mut readers, &mut writers, None, None);
        assert_eq!(result, Ok(1));
        assert_eq!(readers, all_readers);
        assert_eq!(writers, all_writers);
    }

    #[test]
    fn select_on_unreadable_fd() {
        let mut system = VirtualSystem::new();
        let (_reader, writer) = system.pipe().unwrap();
        let mut fds = FdSet::new();
        fds.insert(writer).unwrap();
        let result = system.select(&mut fds, &mut FdSet::new(), None, None);
        assert_eq!(result, Err(Errno::EBADF));
    }

    #[test]
    fn select_on_unwritable_fd() {
        let mut system = VirtualSystem::new();
        let (reader, _writer) = system.pipe().unwrap();
        let mut fds = FdSet::new();
        fds.insert(reader).unwrap();
        let result = system.select(&mut FdSet::new(), &mut fds, None, None);
        assert_eq!(result, Err(Errno::EBADF));
    }

    #[test]
    fn select_on_closed_fd() {
        let mut system = VirtualSystem::new();
        let mut fds = FdSet::new();
        fds.insert(Fd(17)).unwrap();
        let result = system.select(&mut fds, &mut FdSet::new(), None, None);
        assert_eq!(result, Err(Errno::EBADF));

        let result = system.select(&mut FdSet::new(), &mut fds, None, None);
        assert_eq!(result, Err(Errno::EBADF));
    }

    fn system_for_catching_sigchld() -> VirtualSystem {
        let mut system = VirtualSystem::new();
        system
            .sigmask(Some((SigmaskHow::SIG_BLOCK, &[SIGCHLD])), None)
            .unwrap();
        system.sigaction(SIGCHLD, SignalHandling::Catch).unwrap();
        system
    }

    #[test]
    fn select_on_non_pending_signal() {
        let mut system = system_for_catching_sigchld();
        let result = system.select(&mut FdSet::new(), &mut FdSet::new(), None, Some(&[]));
        assert_eq!(result, Ok(0));
        assert_eq!(system.caught_signals(), []);
    }

    #[test]
    fn select_on_pending_signal() {
        let mut system = system_for_catching_sigchld();
        let _ = system.current_process_mut().raise_signal(SIGCHLD);
        let result = system.select(&mut FdSet::new(), &mut FdSet::new(), None, Some(&[]));
        assert_eq!(result, Err(Errno::EINTR));
        assert_eq!(system.caught_signals(), [SIGCHLD]);
    }

    #[test]
    fn select_timeout() {
        let mut system = VirtualSystem::new();
        let now = Instant::now();
        system.state.borrow_mut().now = Some(now);

        let (reader, _writer) = system.pipe().unwrap();
        let mut readers = FdSet::new();
        let mut writers = FdSet::new();
        readers.insert(reader).unwrap();
        let timeout = Duration::new(42, 195).into();

        let result = system.select(&mut readers, &mut writers, Some(&timeout), None);
        assert_eq!(result, Ok(0));
        assert_eq!(readers, FdSet::new());
        assert_eq!(writers, FdSet::new());
        assert_eq!(
            system.state.borrow().now,
            Some(now + Duration::new(42, 195))
        );
    }

    fn virtual_system_with_executor() -> (VirtualSystem, LocalPool) {
        let system = VirtualSystem::new();
        let executor = LocalPool::new();
        system.state.borrow_mut().executor = Some(Rc::new(executor.spawner()));
        (system, executor)
    }

    #[test]
    fn setpgid_creating_new_group_from_parent() {
        let (system, mut executor) = virtual_system_with_executor();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let child = env.system.new_child_process().unwrap();
        let future = child(&mut env, Box::new(|_env| Box::pin(pending())));
        let pid = executor.run_until(future);
        executor.run_until_stalled();

        let result = env.system.setpgid(pid, pid);
        assert_eq!(result, Ok(()));

        let pgid = state.borrow().processes[&pid].pgid();
        assert_eq!(pgid, pid);
    }

    #[test]
    fn setpgid_creating_new_group_from_child() {
        let (system, mut executor) = virtual_system_with_executor();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let child = env.system.new_child_process().unwrap();
        let future = child(
            &mut env,
            Box::new(|child_env| {
                Box::pin(async move {
                    let result = child_env.system.setpgid(Pid(0), Pid(0));
                    assert_eq!(result, Ok(()));
                })
            }),
        );
        let pid = executor.run_until(future);
        executor.run_until_stalled();

        let pgid = state.borrow().processes[&pid].pgid();
        assert_eq!(pgid, pid);
    }

    #[test]
    fn setpgid_extending_existing_group_from_parent() {
        let (system, mut executor) = virtual_system_with_executor();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let child_1 = env.system.new_child_process().unwrap();
        let future = child_1(&mut env, Box::new(|_env| Box::pin(pending())));
        let pid_1 = executor.run_until(future);
        env.system.setpgid(pid_1, pid_1).unwrap();
        let child_2 = env.system.new_child_process().unwrap();
        let future = child_2(&mut env, Box::new(|_env| Box::pin(pending())));
        let pid_2 = executor.run_until(future);
        executor.run_until_stalled();

        let result = env.system.setpgid(pid_2, pid_1);
        assert_eq!(result, Ok(()));

        let pgid = state.borrow().processes[&pid_2].pgid();
        assert_eq!(pgid, pid_1);
    }

    #[test]
    fn setpgid_with_nonexisting_pid() {
        let (system, mut executor) = virtual_system_with_executor();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let child = env.system.new_child_process().unwrap();
        let future = child(&mut env, Box::new(|_env| Box::pin(pending())));
        let pid = executor.run_until(future);
        executor.run_until_stalled();

        let dummy_pid = Pid(123);
        let result = env.system.setpgid(dummy_pid, dummy_pid);
        assert_eq!(result, Err(Errno::ESRCH));

        let pgid = state.borrow().processes[&pid].pgid();
        assert_eq!(pgid, Pid(1));
    }

    #[test]
    fn setpgid_with_unrelated_pid() {
        let (system, mut executor) = virtual_system_with_executor();
        let parent_pid = system.process_id;
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let child = env.system.new_child_process().unwrap();
        let future = child(
            &mut env,
            Box::new(move |child_env| {
                Box::pin(async move {
                    let result = child_env.system.setpgid(parent_pid, Pid(0));
                    assert_eq!(result, Err(Errno::ESRCH));
                })
            }),
        );
        let _pid = executor.run_until(future);
        executor.run_until_stalled();

        let pgid = state.borrow().processes[&parent_pid].pgid();
        assert_eq!(pgid, Pid(1));
    }

    #[test]
    fn setpgid_with_execed_child() {
        let (system, mut executor) = virtual_system_with_executor();
        let path = "/some/file";
        let mut content = INode::default();
        content.body = FileBody::Regular {
            content: vec![],
            is_native_executable: true,
        };
        content.permissions.set(Mode::USER_EXEC, true);
        let content = Rc::new(RefCell::new(content));
        let state = Rc::clone(&system.state);
        state.borrow_mut().file_system.save(path, content).unwrap();
        let mut env = Env::with_system(Box::new(system));
        let child = env.system.new_child_process().unwrap();
        let future = child(
            &mut env,
            Box::new(move |child_env| {
                Box::pin(async move {
                    let path = CString::new(path).unwrap();
                    let _ = child_env.system.execve(&path, &[], &[]);
                })
            }),
        );
        let pid = executor.run_until(future);
        executor.run_until_stalled();

        let result = env.system.setpgid(pid, pid);
        assert_eq!(result, Err(Errno::EACCES));

        let pgid = state.borrow().processes[&pid].pgid();
        assert_eq!(pgid, Pid(1));
    }

    #[test]
    fn setpgid_with_nonexisting_pgid() {
        let (system, mut executor) = virtual_system_with_executor();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let child_1 = env.system.new_child_process().unwrap();
        let future = child_1(&mut env, Box::new(|_env| Box::pin(pending())));
        let pid_1 = executor.run_until(future);
        // env.system.setpgid(pid_1, pid_1).unwrap();
        let child_2 = env.system.new_child_process().unwrap();
        let future = child_2(&mut env, Box::new(|_env| Box::pin(pending())));
        let pid_2 = executor.run_until(future);
        executor.run_until_stalled();

        let result = env.system.setpgid(pid_2, pid_1);
        assert_eq!(result, Err(Errno::EPERM));

        let pgid = state.borrow().processes[&pid_2].pgid();
        assert_eq!(pgid, Pid(1));
    }

    #[test]
    fn tcsetpgrp_success() {
        let mut system = VirtualSystem::new();
        let pid = Pid(10);
        let ppid = system.process_id;
        let pgid = Pid(9);
        system
            .state
            .borrow_mut()
            .processes
            .insert(pid, Process::with_parent_and_group(ppid, pgid));

        system.tcsetpgrp(Fd::STDIN, pgid).unwrap();

        let foreground = system.state.borrow().foreground;
        assert_eq!(foreground, Some(pgid));
    }

    #[test]
    fn tcsetpgrp_with_invalid_fd() {
        let mut system = VirtualSystem::new();
        let result = system.tcsetpgrp(Fd(100), Pid(2));
        assert_eq!(result, Err(Errno::EBADF));
    }

    #[test]
    fn tcsetpgrp_with_nonexisting_pgrp() {
        let mut system = VirtualSystem::new();
        let result = system.tcsetpgrp(Fd::STDIN, Pid(100));
        assert_eq!(result, Err(Errno::EPERM));
    }

    #[test]
    fn new_child_process_without_executor() {
        let mut system = VirtualSystem::new();
        let result = system.new_child_process();
        match result {
            Ok(_) => panic!("unexpected Ok value"),
            Err(e) => assert_eq!(e, Errno::ENOSYS),
        }
    }

    #[test]
    fn new_child_process_with_executor() {
        let (mut system, mut executor) = virtual_system_with_executor();

        let result = system.new_child_process();

        let state = system.state.borrow();
        assert_eq!(state.processes.len(), 2);
        drop(state);

        let mut env = Env::with_system(Box::new(system));
        let child_process = result.unwrap();
        let future = child_process(&mut env, Box::new(|_env| Box::pin(async {})));
        let pid = executor.run_until(future);
        assert_eq!(pid, Pid(3));
    }

    #[test]
    fn wait_for_running_child() {
        let (mut system, mut executor) = virtual_system_with_executor();

        let child_process = system.new_child_process();

        let mut env = Env::with_system(Box::new(system));
        let child_process = child_process.unwrap();
        let future = child_process(&mut env, Box::new(|_env| Box::pin(async move {})));
        let pid = executor.run_until(future);

        let result = env.system.wait(pid);
        assert_eq!(result, Ok(None))
    }

    #[test]
    fn wait_for_exited_child() {
        let (mut system, mut executor) = virtual_system_with_executor();

        let child_process = system.new_child_process();

        let mut env = Env::with_system(Box::new(system));
        let child_process = child_process.unwrap();
        let future = child_process(
            &mut env,
            Box::new(|env| {
                Box::pin(async move {
                    env.exit_status = ExitStatus(5);
                })
            }),
        );
        let pid = executor.run_until(future);
        executor.run_until_stalled();

        let result = env.system.wait(pid);
        assert_eq!(result, Ok(Some((pid, ProcessState::exited(5)))));
    }

    #[test]
    fn wait_for_signaled_child() {
        let (mut system, mut executor) = virtual_system_with_executor();

        let child_process = system.new_child_process();

        let mut env = Env::with_system(Box::new(system));
        let child_process = child_process.unwrap();
        let future = child_process(
            &mut env,
            Box::new(|env| {
                Box::pin(async move {
                    let pid = env.system.getpid();
                    let result = env.system.kill(pid, Some(SIGKILL)).await;
                    unreachable!("kill returned {result:?}");
                })
            }),
        );
        let pid = executor.run_until(future);
        executor.run_until_stalled();

        let result = env.system.wait(pid);
        assert_eq!(
            result,
            Ok(Some((
                pid,
                ProcessState::Halted(ProcessResult::Signaled {
                    signal: SIGKILL,
                    core_dump: false
                })
            )))
        );
    }

    #[test]
    fn wait_for_stopped_child() {
        let (mut system, mut executor) = virtual_system_with_executor();

        let child_process = system.new_child_process();

        let mut env = Env::with_system(Box::new(system));
        let child_process = child_process.unwrap();
        let future = child_process(
            &mut env,
            Box::new(|env| {
                Box::pin(async move {
                    let pid = env.system.getpid();
                    let result = env.system.kill(pid, Some(SIGSTOP)).await;
                    unreachable!("kill returned {result:?}");
                })
            }),
        );
        let pid = executor.run_until(future);
        executor.run_until_stalled();

        let result = env.system.wait(pid);
        assert_eq!(result, Ok(Some((pid, ProcessState::stopped(SIGSTOP)))));
    }

    #[test]
    fn wait_for_resumed_child() {
        let (mut system, mut executor) = virtual_system_with_executor();

        let child_process = system.new_child_process();

        let mut env = Env::with_system(Box::new(system));
        let child_process = child_process.unwrap();
        let future = child_process(
            &mut env,
            Box::new(|env| {
                Box::pin(async move {
                    let pid = env.system.getpid();
                    let result = env.system.kill(pid, Some(SIGSTOP)).await;
                    assert_eq!(result, Ok(()));
                    env.exit_status = ExitStatus(123);
                })
            }),
        );
        let pid = executor.run_until(future);
        executor.run_until_stalled();

        env.system
            .kill(pid, Some(SIGCONT))
            .now_or_never()
            .unwrap()
            .unwrap();

        let result = env.system.wait(pid);
        assert_eq!(result, Ok(Some((pid, ProcessState::Running))));

        executor.run_until_stalled();

        let result = env.system.wait(pid);
        assert_eq!(result, Ok(Some((pid, ProcessState::exited(123)))));
    }

    #[test]
    fn wait_without_child() {
        let mut system = VirtualSystem::new();
        let result = system.wait(Pid::ALL);
        assert_eq!(result, Err(Errno::ECHILD));
        // TODO
        // let result = system.wait(Pid::MY_PROCESS_GROUP);
        // assert_eq!(result, Err(Errno::ECHILD));
        let result = system.wait(system.process_id);
        assert_eq!(result, Err(Errno::ECHILD));
        let result = system.wait(Pid(1234));
        assert_eq!(result, Err(Errno::ECHILD));
        // TODO
        // let result = system.wait(Pid(-1234));
        // assert_eq!(result, Err(Errno::ECHILD));
    }

    #[test]
    fn exiting_child_sends_sigchld_to_parent() {
        let (mut system, mut executor) = virtual_system_with_executor();
        system.sigaction(SIGCHLD, SignalHandling::Catch).unwrap();

        let child_process = system.new_child_process().unwrap();

        let mut env = Env::with_system(Box::new(system));
        let future = child_process(&mut env, Box::new(|_env| Box::pin(async {})));
        executor.run_until(future);
        executor.run_until_stalled();

        assert_eq!(env.system.caught_signals(), [SIGCHLD]);
    }

    #[test]
    fn execve_returns_enosys_for_executable_file() {
        let mut system = VirtualSystem::new();
        let path = "/some/file";
        let mut content = INode::default();
        content.body = FileBody::Regular {
            content: vec![],
            is_native_executable: true,
        };
        content.permissions.set(Mode::USER_EXEC, true);
        let content = Rc::new(RefCell::new(content));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);
        let path = CString::new(path).unwrap();
        let result = system.execve(&path, &[], &[]);
        assert_eq!(result, Err(Errno::ENOSYS));
    }

    #[test]
    fn execve_saves_arguments() {
        let mut system = VirtualSystem::new();
        let path = "/some/file";
        let mut content = INode::default();
        content.body = FileBody::Regular {
            content: vec![],
            is_native_executable: true,
        };
        content.permissions.set(Mode::USER_EXEC, true);
        let content = Rc::new(RefCell::new(content));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);
        let path = CString::new(path).unwrap();
        let args = [c"file".to_owned(), c"bar".to_owned()];
        let envs = [c"foo=FOO".to_owned(), c"baz".to_owned()];
        let _ = system.execve(&path, &args, &envs);

        let process = system.current_process();
        let arguments = process.last_exec.as_ref().unwrap();
        assert_eq!(arguments.0, path);
        assert_eq!(arguments.1, args);
        assert_eq!(arguments.2, envs);
    }

    #[test]
    fn execve_returns_enoexec_for_non_executable_file() {
        let mut system = VirtualSystem::new();
        let path = "/some/file";
        let mut content = INode::default();
        content.permissions.set(Mode::USER_EXEC, true);
        let content = Rc::new(RefCell::new(content));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);
        let path = CString::new(path).unwrap();
        let result = system.execve(&path, &[], &[]);
        assert_eq!(result, Err(Errno::ENOEXEC));
    }

    #[test]
    fn execve_returns_enoent_on_file_not_found() {
        let mut system = VirtualSystem::new();
        let result = system.execve(c"/no/such/file", &[], &[]);
        assert_eq!(result, Err(Errno::ENOENT));
    }

    #[test]
    fn chdir_changes_directory() {
        let mut system = VirtualSystem::new();

        // Create a regular file and its parent directory
        let _ = system.open(
            c"/dir/file",
            OfdAccess::WriteOnly,
            OpenFlag::Create.into(),
            Mode::empty(),
        );

        let result = system.chdir(c"/dir");
        assert_eq!(result, Ok(()));
        assert_eq!(system.current_process().cwd, Path::new("/dir"));
    }

    #[test]
    fn chdir_fails_with_non_existing_directory() {
        let mut system = VirtualSystem::new();

        let result = system.chdir(c"/no/such/dir");
        assert_eq!(result, Err(Errno::ENOENT));
    }

    #[test]
    fn chdir_fails_with_non_directory_file() {
        let mut system = VirtualSystem::new();

        // Create a regular file and its parent directory
        let _ = system.open(
            c"/dir/file",
            OfdAccess::WriteOnly,
            OpenFlag::Create.into(),
            Mode::empty(),
        );

        let result = system.chdir(c"/dir/file");
        assert_eq!(result, Err(Errno::ENOTDIR));
    }

    #[test]
    fn getrlimit_for_unset_resource_returns_infinity() {
        let system = VirtualSystem::new();
        let result = system.getrlimit(Resource::CPU).unwrap();
        assert_eq!(
            result,
            LimitPair {
                soft: RLIM_INFINITY,
                hard: RLIM_INFINITY,
            },
        );
    }

    #[test]
    fn setrlimit_and_getrlimit_with_finite_limits() {
        let mut system = VirtualSystem::new();
        system
            .setrlimit(
                Resource::CORE,
                LimitPair {
                    soft: 4096,
                    hard: 8192,
                },
            )
            .unwrap();
        system
            .setrlimit(Resource::CPU, LimitPair { soft: 10, hard: 30 })
            .unwrap();

        let result = system.getrlimit(Resource::CORE).unwrap();
        assert_eq!(
            result,
            LimitPair {
                soft: 4096,
                hard: 8192,
            },
        );
        let result = system.getrlimit(Resource::CPU).unwrap();
        assert_eq!(result, LimitPair { soft: 10, hard: 30 },);
    }

    #[test]
    fn setrlimit_rejects_soft_limit_higher_than_hard_limit() {
        let mut system = VirtualSystem::new();
        let result = system.setrlimit(Resource::CPU, LimitPair { soft: 2, hard: 1 });
        let error = result.unwrap_err();
        assert_eq!(error.raw_os_error(), Some(nix::libc::EINVAL));

        // The limits should not have been changed
        let result = system.getrlimit(Resource::CPU).unwrap();
        assert_eq!(
            result,
            LimitPair {
                soft: RLIM_INFINITY,
                hard: RLIM_INFINITY,
            },
        );
    }

    #[test]
    fn setrlimit_refuses_raising_hard_limit() {
        let mut system = VirtualSystem::new();
        system
            .setrlimit(Resource::CPU, LimitPair { soft: 1, hard: 1 })
            .unwrap();
        let result = system.setrlimit(Resource::CPU, LimitPair { soft: 1, hard: 2 });
        let error = result.unwrap_err();
        assert_eq!(error.raw_os_error(), Some(nix::libc::EPERM));

        // The limits should not have been changed
        let result = system.getrlimit(Resource::CPU).unwrap();
        assert_eq!(result, LimitPair { soft: 1, hard: 1 });
    }
}
