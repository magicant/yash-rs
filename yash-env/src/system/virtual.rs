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

//! System simulated in Rust
//!
//! [`VirtualSystem`] is a pure Rust implementation of system traits that
//! simulates the behavior of the underlying system without any interaction with
//! the actual system. `VirtualSystem` is used for testing the behavior of the
//! shell in unit tests.
//!
//! # Features and components
//!
//! This module also defines elements that compose a virtual system.
//!
//! ## File system
//!
//! Basic file operations are supported in the virtual system. Regular files,
//! directories, named pipes, and symbolic links can be created in the file
//! system. The file system is shared among all processes in the system.
//!
//! ## Processes
//!
//! A virtual system initially has one process, but can have more processes as a
//! result of simulating fork. Each process has its own state.
//!
//! ## I/O
//!
//! Currently, read and write operations on files and unnamed pipes are
//! supported.
//!
//! ## Signals
//!
//! The virtual system can simulate sending signals to processes. Processes can
//! block, ignore, and catch signals.
//!
//! # Concurrency
//!
//! `VirtualSystem` is designed to allow multiple virtual processes to run
//! concurrently. To achieve this, some trait methods return futures that enable
//! the executor to suspend the calling thread until the process is ready to
//! proceed. This allows the executor to switch between multiple virtual
//! processes on a single thread.
//! See also the [`system` module](super) documentation.
//!
//! TBD: Explain how to use `VirtualSystem` with an executor.

mod file_system;
mod io;
mod process;
mod signal;

pub use self::file_system::*;
pub use self::io::*;
pub use self::process::*;
pub use self::signal::*;
use super::AT_FDCWD;
use super::CaughtSignals;
use super::Chdir;
use super::Clock;
use super::Close;
use super::CpuTimes;
use super::Dir;
use super::Disposition;
use super::Dup;
use super::Errno;
use super::Exec;
use super::Exit;
use super::Fcntl;
use super::FdFlag;
use super::Fork;
use super::Fstat;
use super::GetCwd;
use super::GetPid;
use super::GetPw;
use super::GetRlimit;
use super::GetSigaction;
use super::GetUid;
use super::Gid;
use super::IsExecutableFile;
use super::Isatty;
use super::OfdAccess;
use super::Open;
use super::OpenFlag;
use super::Pipe;
use super::Read;
use super::Result;
use super::Seek;
use super::Select;
use super::SendSignal;
use super::SetPgid;
use super::SetRlimit;
use super::ShellPath;
use super::Sigaction;
use super::Sigmask;
use super::SigmaskOp;
use super::Signals;
use super::Sysconf;
use super::TcGetPgrp;
use super::TcSetPgrp;
use super::Times;
use super::Uid;
use super::Umask;
use super::Wait;
use super::Write;
use super::resource::INFINITY;
use super::resource::LimitPair;
use super::resource::Resource;
#[cfg(doc)]
use crate::System;
use crate::io::Fd;
use crate::job::Pid;
use crate::job::ProcessState;
use crate::path::Path;
use crate::path::PathBuf;
use crate::semantics::ExitStatus;
use crate::str::UnixStr;
use crate::str::UnixString;
use crate::system::ChildProcessStarter;
use enumset::EnumSet;
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
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::c_int;
use std::fmt::Debug;
use std::future::pending;
use std::future::poll_fn;
use std::future::ready;
use std::io::SeekFrom;
use std::ops::DerefMut as _;
use std::ops::RangeInclusive;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use std::time::Duration;
use std::time::Instant;

/// Simulated system
///
/// See the [module-level documentation](self) to grasp a basic understanding of
/// `VirtualSystem`.
///
/// A `VirtualSystem` instance has two members: `state` and `process_id`. The
/// former is a [`SystemState`] that effectively contains the whole state of the
/// system. The state is contained in `Rc` so that virtual processes can share
/// the same state. The latter is a process ID that identifies a process calling
/// the system interfaces.
///
/// When you clone a virtual system, the clone will have the same `process_id`
/// and `state` as the original. To simulate the `fork` system call, you should
/// call a method of the [`Fork`] trait implemented by `VirtualSystem`.
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
            let file = Rc::new(RefCell::new(Inode::new([])));
            state.file_system.save(path, Rc::clone(&file)).unwrap();
            let body = FdBody {
                open_file_description: Rc::new(RefCell::new(OpenFileDescription {
                    file,
                    offset: 0,
                    is_readable: true,
                    is_writable: true,
                    is_appending: true,
                })),
                flags: EnumSet::empty(),
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
                Rc::new(RefCell::new(Inode {
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
    pub fn current_process_mut(&self) -> RefMut<'_, Process> {
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
    pub fn with_open_file_description_mut<F, R>(&self, fd: Fd, f: F) -> Result<R>
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
        follow_symlinks: bool,
    ) -> Result<Rc<RefCell<Inode>>> {
        // TODO Resolve relative to dir_fd
        // TODO Support AT_FDCWD
        const _POSIX_SYMLOOP_MAX: i32 = 8;

        let mut path = Cow::Borrowed(path);
        for _count in 0.._POSIX_SYMLOOP_MAX {
            let resolved_path = self.resolve_relative_path(&path);
            let inode = self.state.borrow().file_system.get(&resolved_path)?;
            if !follow_symlinks {
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

    /// Resolves a file for opening.
    ///
    /// This is a helper function used internally by [`Self::open`], etc.
    fn resolve_file(
        &self,
        path: &CStr,
        access: OfdAccess,
        flags: EnumSet<OpenFlag>,
        mode: Mode,
    ) -> Result<(Rc<RefCell<Inode>>, bool, bool)> {
        let path = self.resolve_relative_path(Path::new(UnixStr::from_bytes(path.to_bytes())));
        let umask = self.current_process().umask;

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
                let mut inode = Inode::new([]);
                inode.permissions = mode.difference(umask);
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

        Ok((file, is_readable, is_writable))
    }

    /// Creates a new file descriptor in the current process.
    ///
    /// This is a helper function used internally by [`Self::open`], etc.
    fn create_fd(
        &self,
        file: Rc<RefCell<Inode>>,
        flags: EnumSet<OpenFlag>,
        is_readable: bool,
        is_writable: bool,
    ) -> std::result::Result<Fd, Errno> {
        let open_file_description = Rc::new(RefCell::new(OpenFileDescription {
            file,
            offset: 0,
            is_readable,
            is_writable,
            is_appending: flags.contains(OpenFlag::Append),
        }));
        let body = FdBody {
            open_file_description,
            flags: if flags.contains(OpenFlag::CloseOnExec) {
                EnumSet::only(FdFlag::CloseOnExec)
            } else {
                EnumSet::empty()
            },
        };
        self.current_process_mut()
            .open_fd(body)
            .map_err(|_| Errno::EMFILE)
    }
}

impl Default for VirtualSystem {
    fn default() -> Self {
        VirtualSystem::new()
    }
}

impl Fstat for VirtualSystem {
    type Stat = Stat;

    fn fstat(&self, fd: Fd) -> Result<Stat> {
        self.with_open_file_description(fd, |ofd| Ok(ofd.file.borrow().stat()))
    }

    fn fstatat(&self, dir_fd: Fd, path: &CStr, follow_symlinks: bool) -> Result<Stat> {
        let path = Path::new(UnixStr::from_bytes(path.to_bytes()));
        let inode = self.resolve_existing_file(dir_fd, path, follow_symlinks)?;
        Ok(inode.borrow().stat())
    }
}

impl IsExecutableFile for VirtualSystem {
    /// Tests whether the specified file is executable or not.
    ///
    /// The current implementation only checks if the file has any executable
    /// bit in the permissions. The file owner and group are not considered.
    fn is_executable_file(&self, path: &CStr) -> bool {
        let path = Path::new(UnixStr::from_bytes(path.to_bytes()));
        self.resolve_existing_file(AT_FDCWD, path, /* follow symlinks */ true)
            .is_ok_and(|inode| inode.borrow().permissions.intersects(Mode::ALL_EXEC))
    }
}

impl Pipe for VirtualSystem {
    fn pipe(&self) -> Result<(Fd, Fd)> {
        let file = Rc::new(RefCell::new(Inode {
            body: FileBody::Fifo {
                content: VecDeque::new(),
                readers: 1,
                writers: 1,
                awaiters: Vec::new(),
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
            flags: EnumSet::empty(),
        };
        let writer = FdBody {
            open_file_description: Rc::new(RefCell::new(writer)),
            flags: EnumSet::empty(),
        };

        let mut process = self.current_process_mut();
        let reader = process.open_fd(reader).map_err(|_| Errno::EMFILE)?;
        let writer = process.open_fd(writer).map_err(|_| {
            process.close_fd(reader);
            Errno::EMFILE
        })?;
        Ok((reader, writer))
    }
}

impl Dup for VirtualSystem {
    fn dup(&self, from: Fd, to_min: Fd, flags: EnumSet<FdFlag>) -> Result<Fd> {
        let mut process = self.current_process_mut();
        let mut body = process.fds.get(&from).ok_or(Errno::EBADF)?.clone();
        body.flags = flags;
        process.open_fd_ge(to_min, body).map_err(|_| Errno::EMFILE)
    }

    fn dup2(&self, from: Fd, to: Fd) -> Result<Fd> {
        let mut process = self.current_process_mut();
        let mut body = process.fds.get(&from).ok_or(Errno::EBADF)?.clone();
        body.flags = EnumSet::empty();
        process.set_fd(to, body).map_err(|_| Errno::EBADF)?;
        Ok(to)
    }
}

impl Open for VirtualSystem {
    /// Opens a file and returns a new file descriptor for it.
    ///
    /// The returned future will be pending until the file is ready to be
    /// opened. For example, if the file is a FIFO, the future will be pending
    /// until the other end of the FIFO is opened, unless `OpenFlag::NonBlock`
    /// is specified.
    fn open(
        &self,
        path: &CStr,
        access: OfdAccess,
        flags: EnumSet<OpenFlag>,
        mode: Mode,
    ) -> impl Future<Output = Result<Fd>> + use<> {
        let resolution = self.resolve_file(path, access, flags, mode);
        let system = self.clone();

        async move {
            let (file, is_readable, is_writable) = resolution?;

            if let FileBody::Fifo {
                readers, writers, ..
            } = &mut file.borrow_mut().body
            {
                // POSIX: Opening a FIFO with O_NONBLOCK | O_WRONLY should fail with
                // ENXIO if there are no readers
                if flags.contains(OpenFlag::NonBlock)
                    && is_writable
                    && !is_readable
                    && *readers == 0
                {
                    return Err(Errno::ENXIO);
                }

                if is_readable {
                    *readers += 1;
                }
                if is_writable {
                    *writers += 1;
                }
            }

            if !flags.contains(OpenFlag::NonBlock) {
                // If the file is a FIFO, block until the other end is opened.
                poll_fn(|context| {
                    let mut file = file.borrow_mut();
                    let FileBody::Fifo {
                        readers,
                        writers,
                        ref mut awaiters,
                        ..
                    } = file.body
                    else {
                        return Poll::Ready(());
                    };

                    if readers == 0 || writers == 0 {
                        // Register the current task as an awaiter if it's not already registered.
                        let waker = context.waker();
                        if !awaiters.iter().any(|existing| existing.will_wake(waker)) {
                            awaiters.push(waker.clone());
                        }

                        Poll::Pending
                    } else {
                        // Wake all awaiters when the FIFO becomes ready.
                        let awaiters = std::mem::take(awaiters);
                        drop(file); // Avoid potential double borrow
                        for task in awaiters {
                            task.wake();
                        }

                        Poll::Ready(())
                    }
                })
                .await;
            }

            system.create_fd(file, flags, is_readable, is_writable)
        }
    }

    fn open_tmpfile(&self, _parent_dir: &Path) -> Result<Fd> {
        let file = Rc::new(RefCell::new(Inode::new([])));
        let open_file_description = Rc::new(RefCell::new(OpenFileDescription {
            file,
            offset: 0,
            is_readable: true,
            is_writable: true,
            is_appending: false,
        }));
        let body = FdBody {
            open_file_description,
            flags: EnumSet::empty(),
        };
        self.current_process_mut()
            .open_fd(body)
            .map_err(|_| Errno::EMFILE)
    }

    fn fdopendir(&self, fd: Fd) -> Result<impl Dir + use<>> {
        self.with_open_file_description(fd, |ofd| {
            let inode = ofd.inode();
            let dir = VirtualDir::try_from(&inode.borrow().body)?;
            Ok(dir)
        })
    }

    fn opendir(&self, path: &CStr) -> Result<impl Dir + use<>> {
        let (file, is_readable, is_writable) = self.resolve_file(
            path,
            OfdAccess::ReadOnly,
            OpenFlag::Directory.into(),
            Mode::empty(),
        )?;

        debug_assert!(
            matches!(file.borrow().body, FileBody::Directory { .. }),
            "resolved file is not a directory"
        );

        let fd = self.create_fd(file, OpenFlag::Directory.into(), is_readable, is_writable)?;
        self.fdopendir(fd)
    }
}

impl Close for VirtualSystem {
    fn close(&self, fd: Fd) -> Result<()> {
        self.current_process_mut().close_fd(fd);
        Ok(())
    }
}

impl Fcntl for VirtualSystem {
    fn ofd_access(&self, fd: Fd) -> Result<OfdAccess> {
        fn is_directory(file_body: &FileBody) -> bool {
            matches!(file_body, FileBody::Directory { .. })
        }

        self.with_open_file_description(fd, |ofd| match (ofd.is_readable, ofd.is_writable) {
            (true, false) => Ok(OfdAccess::ReadOnly),
            (false, true) => Ok(OfdAccess::WriteOnly),
            (true, true) => Ok(OfdAccess::ReadWrite),
            (false, false) => {
                if is_directory(&ofd.inode().borrow().body) {
                    Ok(OfdAccess::Search)
                } else {
                    Ok(OfdAccess::Exec)
                }
            }
        })
    }

    fn get_and_set_nonblocking(&self, fd: Fd, _nonblocking: bool) -> Result<bool> {
        self.with_open_file_description(fd, |_ofd| {
            // TODO Implement non-blocking I/O
            Ok(false)
        })
    }

    fn fcntl_getfd(&self, fd: Fd) -> Result<EnumSet<FdFlag>> {
        let process = self.current_process();
        let body = process.get_fd(fd).ok_or(Errno::EBADF)?;
        Ok(body.flags)
    }

    fn fcntl_setfd(&self, fd: Fd, flags: EnumSet<FdFlag>) -> Result<()> {
        let mut process = self.current_process_mut();
        let body = process.get_fd_mut(fd).ok_or(Errno::EBADF)?;
        body.flags = flags;
        Ok(())
    }
}

impl Read for VirtualSystem {
    fn read(&self, fd: Fd, buffer: &mut [u8]) -> Result<usize> {
        self.with_open_file_description_mut(fd, |ofd| ofd.read(buffer))
    }
}

impl Write for VirtualSystem {
    fn write(&self, fd: Fd, buffer: &[u8]) -> Result<usize> {
        self.with_open_file_description_mut(fd, |ofd| ofd.write(buffer))
    }
}

impl Seek for VirtualSystem {
    fn lseek(&self, fd: Fd, position: SeekFrom) -> Result<u64> {
        self.with_open_file_description_mut(fd, |ofd| ofd.seek(position))
            .and_then(|new_offset| new_offset.try_into().map_err(|_| Errno::EOVERFLOW))
    }
}

impl Umask for VirtualSystem {
    fn umask(&self, new_mask: Mode) -> Mode {
        std::mem::replace(&mut self.current_process_mut().umask, new_mask)
    }
}

impl GetCwd for VirtualSystem {
    fn getcwd(&self) -> Result<PathBuf> {
        Ok(self.current_process().cwd.clone())
    }
}

impl Chdir for VirtualSystem {
    fn chdir(&self, path: &CStr) -> Result<()> {
        let path = Path::new(UnixStr::from_bytes(path.to_bytes()));
        let inode = self.resolve_existing_file(AT_FDCWD, path, /* follow links */ true)?;
        if matches!(&inode.borrow().body, FileBody::Directory { .. }) {
            let mut process = self.current_process_mut();
            let new_path = process.cwd.join(path);
            process.chdir(new_path);
            Ok(())
        } else {
            Err(Errno::ENOTDIR)
        }
    }
}

impl Clock for VirtualSystem {
    /// Returns `now` in [`SystemState`].
    ///
    /// Panics if it is `None`.
    fn now(&self) -> Instant {
        self.state
            .borrow()
            .now
            .expect("SystemState::now not assigned")
    }
}

impl Times for VirtualSystem {
    /// Returns `times` in [`SystemState`].
    fn times(&self) -> Result<CpuTimes> {
        Ok(self.state.borrow().times)
    }
}

impl Signals for VirtualSystem {
    const SIGABRT: signal::Number = signal::SIGABRT;
    const SIGALRM: signal::Number = signal::SIGALRM;
    const SIGBUS: signal::Number = signal::SIGBUS;
    const SIGCHLD: signal::Number = signal::SIGCHLD;
    const SIGCLD: Option<signal::Number> = Some(signal::SIGCLD);
    const SIGCONT: signal::Number = signal::SIGCONT;
    const SIGEMT: Option<signal::Number> = Some(signal::SIGEMT);
    const SIGFPE: signal::Number = signal::SIGFPE;
    const SIGHUP: signal::Number = signal::SIGHUP;
    const SIGILL: signal::Number = signal::SIGILL;
    const SIGINFO: Option<signal::Number> = Some(signal::SIGINFO);
    const SIGINT: signal::Number = signal::SIGINT;
    const SIGIO: Option<signal::Number> = Some(signal::SIGIO);
    const SIGIOT: signal::Number = signal::SIGIOT;
    const SIGKILL: signal::Number = signal::SIGKILL;
    const SIGLOST: Option<signal::Number> = Some(signal::SIGLOST);
    const SIGPIPE: signal::Number = signal::SIGPIPE;
    const SIGPOLL: Option<signal::Number> = Some(signal::SIGPOLL);
    const SIGPROF: signal::Number = signal::SIGPROF;
    const SIGPWR: Option<signal::Number> = Some(signal::SIGPWR);
    const SIGQUIT: signal::Number = signal::SIGQUIT;
    const SIGSEGV: signal::Number = signal::SIGSEGV;
    const SIGSTKFLT: Option<signal::Number> = Some(signal::SIGSTKFLT);
    const SIGSTOP: signal::Number = signal::SIGSTOP;
    const SIGSYS: signal::Number = signal::SIGSYS;
    const SIGTERM: signal::Number = signal::SIGTERM;
    const SIGTHR: Option<signal::Number> = Some(signal::SIGTHR);
    const SIGTRAP: signal::Number = signal::SIGTRAP;
    const SIGTSTP: signal::Number = signal::SIGTSTP;
    const SIGTTIN: signal::Number = signal::SIGTTIN;
    const SIGTTOU: signal::Number = signal::SIGTTOU;
    const SIGURG: signal::Number = signal::SIGURG;
    const SIGUSR1: signal::Number = signal::SIGUSR1;
    const SIGUSR2: signal::Number = signal::SIGUSR2;
    const SIGVTALRM: signal::Number = signal::SIGVTALRM;
    const SIGWINCH: signal::Number = signal::SIGWINCH;
    const SIGXCPU: signal::Number = signal::SIGXCPU;
    const SIGXFSZ: signal::Number = signal::SIGXFSZ;

    fn sigrt_range(&self) -> Option<RangeInclusive<Number>> {
        Some(signal::SIGRTMIN..=signal::SIGRTMAX)
    }
}

impl GetPid for VirtualSystem {
    /// Currently, this function always returns `Pid(2)` if the process exists.
    fn getsid(&self, pid: Pid) -> Result<Pid> {
        self.state
            .borrow()
            .processes
            .get(&pid)
            .map_or(Err(Errno::ESRCH), |_| Ok(Pid(2)))
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
}

impl SetPgid for VirtualSystem {
    /// Modifies the process group ID of a process.
    ///
    /// The current implementation does not yet support the concept of sessions.
    fn setpgid(&self, mut pid: Pid, mut pgid: Pid) -> Result<()> {
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
}

impl Sigmask for VirtualSystem {
    fn sigmask(
        &self,
        op: Option<(SigmaskOp, &[signal::Number])>,
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

        if let Some((op, mask)) = op {
            let result = process.block_signals(op, mask);
            if result.process_state_changed {
                let parent_pid = process.ppid;
                raise_sigchld(&mut state, parent_pid);
            }
        }

        Ok(())
    }
}

impl GetSigaction for VirtualSystem {
    fn get_sigaction(&self, signal: signal::Number) -> Result<Disposition> {
        let process = self.current_process();
        Ok(process.disposition(signal))
    }
}

impl Sigaction for VirtualSystem {
    fn sigaction(&self, signal: signal::Number, disposition: Disposition) -> Result<Disposition> {
        let mut process = self.current_process_mut();
        Ok(process.set_disposition(signal, disposition))
    }
}

impl CaughtSignals for VirtualSystem {
    fn caught_signals(&self) -> Vec<signal::Number> {
        std::mem::take(&mut self.current_process_mut().caught_signals)
    }
}

impl SendSignal for VirtualSystem {
    /// Sends a signal to the target process.
    ///
    /// The current implementation accepts any positive signal number and `None`
    /// (no signal) for `signal`. Negative signal numbers are rejected with
    /// `Errno::EINVAL`.
    ///
    /// This function returns a future that enables the executor to block the
    /// calling thread until the current process is ready to proceed. If the
    /// signal is sent to the current process and it causes the process to stop,
    /// the future will be ready only when the process is resumed. Similarly, if
    /// the signal causes the current process to terminate, the future will
    /// never be ready.
    fn kill(
        &self,
        target: Pid,
        signal: Option<signal::Number>,
    ) -> impl Future<Output = Result<()>> + use<> {
        let result = 'result: {
            if let Some(signal) = signal {
                // Validate the signal number
                if signal.as_raw() < 0 {
                    break 'result Err(Errno::EINVAL);
                }
            }

            match target {
                Pid::MY_PROCESS_GROUP => {
                    let target_pgid = self.current_process().pgid;
                    send_signal_to_processes(
                        &mut self.state.borrow_mut(),
                        Some(target_pgid),
                        signal,
                    )
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
                    send_signal_to_processes(
                        &mut self.state.borrow_mut(),
                        Some(target_pgid),
                        signal,
                    )
                }
            }
        };

        let system = self.clone();
        async move {
            system.block_until_running().await;
            result
        }
    }

    fn raise(&self, signal: signal::Number) -> impl Future<Output = Result<()>> + use<> {
        let target = self.process_id;
        self.kill(target, Some(signal))
    }
}

impl Select for VirtualSystem {
    /// Waits for a next event.
    ///
    /// The `VirtualSystem` implementation for this method does not actually
    /// block the calling thread. The method returns immediately in any case.
    ///
    /// The `timeout` is ignored if this function returns because of a ready FD
    /// or a caught signal. Otherwise, the timeout is added to
    /// [`SystemState::now`], which must not be `None` then.
    fn select(
        &self,
        readers: &mut Vec<Fd>,
        writers: &mut Vec<Fd>,
        timeout: Option<Duration>,
        signal_mask: Option<&[signal::Number]>,
    ) -> Result<c_int> {
        let mut process = self.current_process_mut();

        // Detect invalid FDs first. POSIX requires that the arguments are
        // not modified if an error occurs.
        let fds = readers.iter().chain(writers.iter());
        if { fds }.any(|fd| !process.fds().contains_key(fd)) {
            return Err(Errno::EBADF);
        }

        if let Some(signal_mask) = signal_mask {
            let save_mask = process
                .blocked_signals()
                .iter()
                .copied()
                .collect::<Vec<signal::Number>>();
            let result_1 = process.block_signals(SigmaskOp::Set, signal_mask);
            let result_2 = process.block_signals(SigmaskOp::Set, &save_mask);
            assert!(!result_2.delivered);
            if result_1.caught {
                return Err(Errno::EINTR);
            }
        }

        readers.retain(|fd| {
            // We already checked that the FD is open, so it's safe to access by index.
            let ofd = process.fds()[fd].open_file_description.borrow();
            !ofd.is_readable() || ofd.is_ready_for_reading()
        });
        writers.retain(|fd| {
            let ofd = process.fds()[fd].open_file_description.borrow();
            !ofd.is_writable() || ofd.is_ready_for_writing()
        });

        drop(process);

        let count = (readers.len() + writers.len()).try_into().unwrap();
        if count == 0 {
            if let Some(duration) = timeout {
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
}

impl Isatty for VirtualSystem {
    fn isatty(&self, fd: Fd) -> bool {
        self.with_open_file_description(fd, |ofd| {
            Ok(matches!(&ofd.file.borrow().body, FileBody::Terminal { .. }))
        })
        .unwrap_or(false)
    }
}

impl TcGetPgrp for VirtualSystem {
    /// Returns the current foreground process group ID.
    ///
    /// The current implementation does not yet support the concept of
    /// controlling terminals and sessions. It accepts any open file descriptor.
    fn tcgetpgrp(&self, fd: Fd) -> Result<Pid> {
        // Make sure the FD is open
        self.with_open_file_description(fd, |_| Ok(()))?;

        self.state.borrow().foreground.ok_or(Errno::ENOTTY)
    }
}

impl TcSetPgrp for VirtualSystem {
    /// Switches the foreground process.
    ///
    /// The current implementation does not yet support the concept of
    /// controlling terminals and sessions. It accepts any open file descriptor.
    fn tcsetpgrp(&self, fd: Fd, pgid: Pid) -> impl Future<Output = Result<()>> + use<> {
        fn inner(system: &VirtualSystem, fd: Fd, pgid: Pid) -> Result<()> {
            // Make sure the FD is open
            system.with_open_file_description(fd, |_| Ok(()))?;

            // Make sure the process group exists
            let mut state = system.state.borrow_mut();
            if !state.processes.values().any(|p| p.pgid == pgid) {
                return Err(Errno::EPERM);
            }

            // TODO: Suspend the calling process group if it is in the background
            // and not ignoring or blocking SIGTTOU.

            state.foreground = Some(pgid);
            Ok(())
        }

        ready(inner(self, fd, pgid))
    }
}

impl Fork for VirtualSystem {
    /// Creates a new child process.
    ///
    /// This implementation does not create any real child process. Instead,
    /// it returns a child process starter that runs its task concurrently in
    /// the same process.
    ///
    /// To run the concurrent task, this function needs an executor that has
    /// been set in the [`SystemState`]. If the system state does not have an
    /// executor, this function fails with `Errno::ENOSYS`.
    ///
    /// The process ID of the child will be the maximum of existing process IDs
    /// plus 1. If there are no other processes, it will be 2.
    fn new_child_process(&self) -> Result<ChildProcessStarter<Self>> {
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
            let system = VirtualSystem { state, process_id };
            let mut child_env = parent_env.clone_with_system(system.clone());

            {
                let mut process = system.current_process_mut();
                process.selector = Rc::downgrade(&child_env.system.0);
            }

            let run_task_and_set_exit_status = Box::pin(async move {
                let runner = ProcessRunner {
                    task: task(&mut child_env),
                    system,
                    waker: Rc::new(Cell::new(None)),
                };
                runner.await;
            });

            executor
                .spawn(run_task_and_set_exit_status)
                .expect("the executor failed to start the child process task");

            process_id
        }))
    }
}

impl Wait for VirtualSystem {
    /// Waits for a child.
    ///
    /// TODO: Currently, this function only supports `target == -1 || target > 0`.
    fn wait(&self, target: Pid) -> Result<Option<(Pid, ProcessState)>> {
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
}

impl Exec for VirtualSystem {
    /// Stub for the `execve` system call.
    ///
    /// The `execve` system call cannot be simulated in the userland. This
    /// function returns `ENOSYS` if the file at `path` is a native executable,
    /// `ENOEXEC` if a non-executable file, and `ENOENT` otherwise.
    fn execve(
        &self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> impl Future<Output = Result<Infallible>> + use<> {
        let os_path = UnixStr::from_bytes(path.to_bytes());
        let mut state = self.state.borrow_mut();
        let fs = &state.file_system;
        let file = match fs.get(os_path) {
            Ok(file) => file,
            Err(e) => return ready(Err(e)),
        };
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

            // TODO: We should abort the currently running task and start the new one.
            // Just returning `pending()` would break existing tests that rely on
            // the current behavior.
            ready(Err(Errno::ENOSYS))
        } else {
            ready(Err(Errno::ENOEXEC))
        }
    }
}

impl Exit for VirtualSystem {
    fn exit(&self, exit_status: ExitStatus) -> impl Future<Output = Infallible> + use<> {
        let mut myself = self.current_process_mut();
        let parent_pid = myself.ppid;
        let exited = myself.set_state(ProcessState::exited(exit_status));
        drop(myself);
        if exited {
            raise_sigchld(&mut self.state.borrow_mut(), parent_pid);
        }

        pending()
    }
}

impl GetUid for VirtualSystem {
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
}

impl GetPw for VirtualSystem {
    fn getpwnam_dir(&self, name: &CStr) -> Result<Option<PathBuf>> {
        let state = self.state.borrow();
        let name = match name.to_str() {
            Ok(name) => name,
            Err(_utf8_error) => return Ok(None),
        };
        Ok(state.home_dirs.get(name).cloned())
    }
}

impl Sysconf for VirtualSystem {
    /// Returns the standard path for the system.
    ///
    /// This function returns the value of [`SystemState::path`]. If it is empty,
    /// it returns the `ENOSYS` error.
    fn confstr_path(&self) -> Result<UnixString> {
        let path = self.state.borrow().path.clone();
        if path.is_empty() {
            Err(Errno::ENOSYS)
        } else {
            Ok(path)
        }
    }
}

impl ShellPath for VirtualSystem {
    /// Returns the path to the shell.
    ///
    /// The current implementation returns "/bin/sh".
    fn shell_path(&self) -> CString {
        c"/bin/sh".to_owned()
    }
}

impl GetRlimit for VirtualSystem {
    fn getrlimit(&self, resource: Resource) -> Result<LimitPair> {
        Ok(self
            .current_process()
            .resource_limits
            .get(&resource)
            .copied()
            .unwrap_or(LimitPair {
                soft: INFINITY,
                hard: INFINITY,
            }))
    }
}

impl SetRlimit for VirtualSystem {
    fn setrlimit(&self, resource: Resource, limits: LimitPair) -> Result<()> {
        if limits.soft_exceeds_hard() {
            return Err(Errno::EINVAL);
        }

        let mut process = self.current_process_mut();
        use std::collections::hash_map::Entry::{Occupied, Vacant};
        match process.resource_limits.entry(resource) {
            Occupied(occupied) => {
                let occupied = occupied.into_mut();
                if limits.hard > occupied.hard {
                    return Err(Errno::EPERM);
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

    for (&_pid, process) in &mut state.processes {
        if target_pgid.is_none_or(|target_pgid| process.pgid == target_pgid) {
            let result = if let Some(signal) = signal {
                process.raise_signal(signal)
            } else {
                SignalResult::default()
            };
            results.push((result, process.ppid));
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

    /// Consumed CPU time statistics
    pub times: CpuTimes,

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
    pub path: UnixString,
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
    task: Pin<Box<dyn Future<Output = Infallible> + 'a>>,
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
            match poll {
                // unreachable: Poll::Ready(_) => todo!(),
                Poll::Pending => (),
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
    use crate::Env;
    use crate::job::ProcessResult;
    use crate::system::FileType;
    use assert_matches::assert_matches;
    use futures_executor::LocalPool;
    use futures_util::FutureExt as _;
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
            system.fstatat(Fd(0), c"/no/such/file", true),
            Err(Errno::ENOENT)
        );
    }

    #[test]
    fn fstatat_regular_file() {
        let system = VirtualSystem::new();
        let path = "/some/file";
        let content = Rc::new(RefCell::new(Inode::new([1, 2, 3, 42, 100])));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);

        let stat = system.fstatat(Fd(0), c"/some/file", true).unwrap();
        assert_eq!(stat.mode, Mode::default());
        assert_eq!(stat.r#type, FileType::Regular);
        assert_eq!(stat.size, 5);
        // TODO Other stat properties
    }

    #[test]
    fn fstatat_directory() {
        let system = VirtualSystem::new();
        let path = "/some/file";
        let content = Rc::new(RefCell::new(Inode::new([])));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);

        let stat = system.fstatat(Fd(0), c"/some/", true).unwrap();
        assert_eq!(stat.mode, Mode::from_bits_retain(0o755));
        assert_eq!(stat.r#type, FileType::Directory);
        // TODO Other stat properties
    }

    #[test]
    fn fstatat_fifo() {
        let system = VirtualSystem::new();
        let path = "/some/fifo";
        let content = Rc::new(RefCell::new(Inode {
            body: FileBody::Fifo {
                content: [17; 42].into(),
                readers: 0,
                writers: 0,
                awaiters: Vec::new(),
            },
            permissions: Mode::default(),
        }));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);

        let stat = system.fstatat(Fd(0), c"/some/fifo", true).unwrap();
        assert_eq!(stat.mode, Mode::default());
        assert_eq!(stat.r#type, FileType::Fifo);
        assert_eq!(stat.size, 42);
    }

    fn system_with_symlink() -> VirtualSystem {
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();
        state
            .file_system
            .save("/some/file", Rc::new(RefCell::new(Inode::new([]))))
            .unwrap();
        state
            .file_system
            .save(
                "/link",
                Rc::new(RefCell::new(Inode {
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
        let stat = system.fstatat(Fd(0), c"/link", true).unwrap();
        assert_eq!(stat.r#type, FileType::Regular);
    }

    #[test]
    fn fstatat_symlink_no_follow() {
        let system = system_with_symlink();
        let stat = system.fstatat(Fd(0), c"/link", false).unwrap();
        assert_eq!(stat.r#type, FileType::Symlink);
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
        let content = Rc::new(RefCell::new(Inode::default()));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);
        assert!(!system.is_executable_file(c"/some/file"));
    }

    #[test]
    fn is_executable_file_with_executable_file() {
        let system = VirtualSystem::new();
        let path = "/some/file";
        let mut content = Inode::default();
        content.permissions.set(Mode::USER_EXEC, true);
        let content = Rc::new(RefCell::new(content));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);
        assert!(system.is_executable_file(c"/some/file"));
    }

    #[test]
    fn pipe_read_write() {
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
        let result = system.dup(Fd::STDOUT, Fd::STDERR, EnumSet::empty());
        assert_eq!(result, Ok(Fd(3)));

        let process = system.current_process();
        let fd1 = process.fds.get(&Fd(1)).unwrap();
        let fd3 = process.fds.get(&Fd(3)).unwrap();
        assert_eq!(fd1, fd3);
    }

    #[test]
    fn dup_can_set_cloexec() {
        let system = VirtualSystem::new();
        let result = system.dup(Fd::STDOUT, Fd::STDERR, FdFlag::CloseOnExec.into());
        assert_eq!(result, Ok(Fd(3)));

        let process = system.current_process();
        let fd3 = process.fds.get(&Fd(3)).unwrap();
        assert_eq!(fd3.flags, EnumSet::only(FdFlag::CloseOnExec));
    }

    #[test]
    fn dup2_shares_open_file_description() {
        let system = VirtualSystem::new();
        let result = system.dup2(Fd::STDOUT, Fd(5));
        assert_eq!(result, Ok(Fd(5)));

        let process = system.current_process();
        let fd1 = process.fds.get(&Fd(1)).unwrap();
        let fd5 = process.fds.get(&Fd(5)).unwrap();
        assert_eq!(fd1, fd5);
    }

    #[test]
    fn dup2_clears_cloexec() {
        let system = VirtualSystem::new();
        let mut process = system.current_process_mut();
        process.fds.get_mut(&Fd::STDOUT).unwrap().flags = FdFlag::CloseOnExec.into();
        drop(process);

        let result = system.dup2(Fd::STDOUT, Fd(6));
        assert_eq!(result, Ok(Fd(6)));

        let process = system.current_process();
        let fd6 = process.fds.get(&Fd(6)).unwrap();
        assert_eq!(fd6.flags, EnumSet::empty());
    }

    #[test]
    fn open_non_existing_file_no_creation() {
        let system = VirtualSystem::new();
        let result = system
            .open(
                c"/no/such/file",
                OfdAccess::ReadOnly,
                EnumSet::empty(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();
        assert_eq!(result, Err(Errno::ENOENT));
    }

    #[test]
    fn open_creating_non_existing_file() {
        let system = VirtualSystem::new();
        let result = system
            .open(
                c"new_file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Fd(3)));

        system.write(Fd(3), &[42, 123]).unwrap();
        let file = system.state.borrow().file_system.get("new_file").unwrap();
        let file = file.borrow();
        assert_eq!(file.permissions, Mode::empty());
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(content[..], [42, 123]);
        });
    }

    #[test]
    fn open_creating_non_existing_file_umask() {
        let system = VirtualSystem::new();
        system.umask(Mode::from_bits_retain(0o125));
        system
            .open(
                c"file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::ALL_9,
            )
            .now_or_never()
            .unwrap()
            .unwrap();

        let file = system.state.borrow().file_system.get("file").unwrap();
        let file = file.borrow();
        assert_eq!(file.permissions, Mode::from_bits_retain(0o652));
    }

    #[test]
    fn open_existing_file() {
        let system = VirtualSystem::new();
        let fd = system
            .open(
                c"file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        system.write(fd, &[75, 96, 133]).unwrap();

        let result = system
            .open(
                c"file",
                OfdAccess::ReadOnly,
                EnumSet::empty(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();
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
        let system = VirtualSystem::new();
        let first = system
            .open(
                c"my_file",
                OfdAccess::WriteOnly,
                OpenFlag::Create | OpenFlag::Exclusive,
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();
        assert_eq!(first, Ok(Fd(3)));

        let second = system
            .open(
                c"my_file",
                OfdAccess::WriteOnly,
                OpenFlag::Create | OpenFlag::Exclusive,
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();
        assert_eq!(second, Err(Errno::EEXIST));
    }

    #[test]
    fn open_truncating() {
        let system = VirtualSystem::new();
        let fd = system
            .open(
                c"file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::ALL_9,
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        system.write(fd, &[1, 2, 3]).unwrap();

        let result = system
            .open(
                c"file",
                OfdAccess::WriteOnly,
                OpenFlag::Truncate.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Fd(4)));

        let reader = system
            .open(
                c"file",
                OfdAccess::ReadOnly,
                EnumSet::empty(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        let count = system.read(reader, &mut [0; 1]).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn open_appending() {
        let system = VirtualSystem::new();
        let fd = system
            .open(
                c"file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::ALL_9,
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        system.write(fd, &[1, 2, 3]).unwrap();

        let result = system
            .open(
                c"file",
                OfdAccess::WriteOnly,
                OpenFlag::Append.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Fd(4)));
        system.write(Fd(4), &[4, 5, 6]).unwrap();

        let reader = system
            .open(
                c"file",
                OfdAccess::ReadOnly,
                EnumSet::empty(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        let mut buffer = [0; 7];
        let count = system.read(reader, &mut buffer).unwrap();
        assert_eq!(count, 6);
        assert_eq!(buffer, [1, 2, 3, 4, 5, 6, 0]);
    }

    #[test]
    fn open_directory() {
        let system = VirtualSystem::new();

        // Create a regular file and its parent directory
        let _ = system
            .open(
                c"/dir/file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();

        let result = system
            .open(
                c"/dir",
                OfdAccess::ReadOnly,
                OpenFlag::Directory.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Fd(4)));
    }

    #[test]
    fn open_non_directory_path_prefix() {
        let system = VirtualSystem::new();

        // Create a regular file
        let _ = system
            .open(
                c"/file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();

        let result = system
            .open(
                c"/file/file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();
        assert_eq!(result, Err(Errno::ENOTDIR));
    }

    #[test]
    fn open_non_directory_file() {
        let system = VirtualSystem::new();

        // Create a regular file
        let _ = system
            .open(
                c"/file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();

        let result = system
            .open(
                c"/file",
                OfdAccess::ReadOnly,
                OpenFlag::Directory.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();
        assert_eq!(result, Err(Errno::ENOTDIR));
    }

    #[test]
    fn open_default_working_directory() {
        // The default working directory is the root directory.
        let system = VirtualSystem::new();

        let writer = system
            .open(
                c"/dir/file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::ALL_9,
            )
            .now_or_never()
            .unwrap();
        system.write(writer.unwrap(), &[1, 2, 3, 42]).unwrap();

        let reader = system
            .open(
                c"./dir/file",
                OfdAccess::ReadOnly,
                EnumSet::empty(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();
        let mut buffer = [0; 10];
        let count = system.read(reader.unwrap(), &mut buffer).unwrap();
        assert_eq!(count, 4);
        assert_eq!(buffer[0..4], [1, 2, 3, 42]);
    }

    #[test]
    fn open_tmpfile() {
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();

        let result = system.close(Fd::STDERR);
        assert_eq!(result, Ok(()));
        assert_eq!(system.current_process().fds.get(&Fd::STDERR), None);

        let result = system.close(Fd::STDERR);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn fcntl_getfd_and_setfd() {
        let system = VirtualSystem::new();

        let flags = system.fcntl_getfd(Fd::STDIN).unwrap();
        assert_eq!(flags, EnumSet::empty());

        system
            .fcntl_setfd(Fd::STDIN, FdFlag::CloseOnExec.into())
            .unwrap();

        let flags = system.fcntl_getfd(Fd::STDIN).unwrap();
        assert_eq!(flags, EnumSet::only(FdFlag::CloseOnExec));

        let flags = system.fcntl_getfd(Fd::STDOUT).unwrap();
        assert_eq!(flags, EnumSet::empty());

        system.fcntl_setfd(Fd::STDIN, EnumSet::empty()).unwrap();

        let flags = system.fcntl_getfd(Fd::STDIN).unwrap();
        assert_eq!(flags, EnumSet::empty());
    }

    #[test]
    fn opendir_default_working_directory() {
        // The default working directory is the root directory.
        let system = VirtualSystem::new();

        let _ = system
            .open(
                c"/dir/file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::ALL_9,
            )
            .now_or_never()
            .unwrap();

        let mut dir = system.opendir(c"./dir").unwrap();
        let mut files = Vec::new();
        while let Some(entry) = dir.next().unwrap() {
            files.push(entry.name.to_unix_string());
        }
        files.sort_unstable();
        assert_eq!(
            files[..],
            [
                UnixString::from("."),
                UnixString::from(".."),
                UnixString::from("file")
            ]
        );
    }

    // TODO Test sigmask

    #[test]
    fn kill_process() {
        let system = VirtualSystem::new();
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

        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
    fn kill_dummy_signal_to_my_group() {
        let system = VirtualSystem::new();

        let result = system
            .kill(Pid::MY_PROCESS_GROUP, None)
            .now_or_never()
            .unwrap();

        assert_eq!(result, Ok(()));
        assert_eq!(system.current_process().state(), ProcessState::Running);
    }

    #[test]
    fn kill_dummy_signal_to_non_existent_group() {
        let system = VirtualSystem::new();
        let result = system.kill(Pid(-9999), None).now_or_never().unwrap();
        assert_eq!(result, Err(Errno::ESRCH));
    }

    #[test]
    fn select_regular_file_is_always_ready() {
        let system = VirtualSystem::new();
        let mut readers = vec![Fd::STDIN];
        let mut writers = vec![Fd::STDOUT, Fd::STDERR];

        let result = system.select(&mut readers, &mut writers, None, None);
        assert_eq!(result, Ok(3));
        assert_eq!(readers, [Fd::STDIN]);
        assert_eq!(writers, [Fd::STDOUT, Fd::STDERR]);
    }

    #[test]
    fn select_pipe_reader_is_ready_if_writer_is_closed() {
        let system = VirtualSystem::new();
        let (reader, writer) = system.pipe().unwrap();
        system.close(writer).unwrap();
        let mut readers = vec![reader];
        let mut writers = vec![];

        let result = system.select(&mut readers, &mut writers, None, None);
        assert_eq!(result, Ok(1));
        assert_eq!(readers, [reader]);
        assert_eq!(writers, []);
    }

    #[test]
    fn select_pipe_reader_is_ready_if_something_has_been_written() {
        let system = VirtualSystem::new();
        let (reader, writer) = system.pipe().unwrap();
        system.write(writer, &[0]).unwrap();
        let mut readers = vec![reader];
        let mut writers = vec![];

        let result = system.select(&mut readers, &mut writers, None, None);
        assert_eq!(result, Ok(1));
        assert_eq!(readers, [reader]);
        assert_eq!(writers, []);
    }

    #[test]
    fn select_pipe_reader_is_not_ready_if_writer_has_written_nothing() {
        let system = VirtualSystem::new();
        let (reader, _writer) = system.pipe().unwrap();
        let mut readers = vec![reader];
        let mut writers = vec![];

        let result = system.select(&mut readers, &mut writers, None, None);
        assert_eq!(result, Ok(0));
        assert_eq!(readers, []);
        assert_eq!(writers, []);
    }

    #[test]
    fn select_pipe_writer_is_ready_if_pipe_is_not_full() {
        let system = VirtualSystem::new();
        let (_reader, writer) = system.pipe().unwrap();
        let mut readers = vec![];
        let mut writers = vec![writer];

        let result = system.select(&mut readers, &mut writers, None, None);
        assert_eq!(result, Ok(1));
        assert_eq!(readers, []);
        assert_eq!(writers, [writer]);
    }

    #[test]
    fn select_on_unreadable_fd() {
        let system = VirtualSystem::new();
        let (_reader, writer) = system.pipe().unwrap();
        let mut fds = vec![writer];
        let result = system.select(&mut fds, &mut vec![], None, None);
        assert_eq!(result, Ok(1));
        assert_eq!(fds, [writer]);
    }

    #[test]
    fn select_on_unwritable_fd() {
        let system = VirtualSystem::new();
        let (reader, _writer) = system.pipe().unwrap();
        let mut fds = vec![reader];
        let result = system.select(&mut vec![], &mut fds, None, None);
        assert_eq!(result, Ok(1));
        assert_eq!(fds, [reader]);
    }

    #[test]
    fn select_on_closed_fd() {
        let system = VirtualSystem::new();
        let result = system.select(&mut vec![Fd(17)], &mut vec![], None, None);
        assert_eq!(result, Err(Errno::EBADF));

        let result = system.select(&mut vec![], &mut vec![Fd(17)], None, None);
        assert_eq!(result, Err(Errno::EBADF));
    }

    fn system_for_catching_sigchld() -> VirtualSystem {
        let system = VirtualSystem::new();
        system
            .sigmask(Some((SigmaskOp::Add, &[SIGCHLD])), None)
            .unwrap();
        system.sigaction(SIGCHLD, Disposition::Catch).unwrap();
        system
    }

    #[test]
    fn select_on_non_pending_signal() {
        let system = system_for_catching_sigchld();
        let result = system.select(&mut vec![], &mut vec![], None, Some(&[]));
        assert_eq!(result, Ok(0));
        assert_eq!(system.caught_signals(), []);
    }

    #[test]
    fn select_on_pending_signal() {
        let system = system_for_catching_sigchld();
        let _ = system.current_process_mut().raise_signal(SIGCHLD);
        let result = system.select(&mut vec![], &mut vec![], None, Some(&[]));
        assert_eq!(result, Err(Errno::EINTR));
        assert_eq!(system.caught_signals(), [SIGCHLD]);
    }

    #[test]
    fn select_timeout() {
        let system = VirtualSystem::new();
        let now = Instant::now();
        system.state.borrow_mut().now = Some(now);

        let (reader, _writer) = system.pipe().unwrap();
        let mut readers = vec![reader];
        let mut writers = vec![];
        let timeout = Duration::new(42, 195);

        let result = system.select(&mut readers, &mut writers, Some(timeout), None);
        assert_eq!(result, Ok(0));
        assert_eq!(readers, []);
        assert_eq!(writers, []);
        assert_eq!(
            system.state.borrow().now,
            Some(now + Duration::new(42, 195))
        );
    }

    pub(super) fn virtual_system_with_executor() -> (VirtualSystem, LocalPool) {
        let system = VirtualSystem::new();
        let executor = LocalPool::new();
        system.state.borrow_mut().executor = Some(Rc::new(executor.spawner()));
        (system, executor)
    }

    #[test]
    fn setpgid_creating_new_group_from_parent() {
        let (system, _executor) = virtual_system_with_executor();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let child = env.system.new_child_process().unwrap();
        let pid = child(&mut env, Box::new(|_env| Box::pin(pending())));

        let result = env.system.setpgid(pid, pid);
        assert_eq!(result, Ok(()));

        let pgid = state.borrow().processes[&pid].pgid();
        assert_eq!(pgid, pid);
    }

    #[test]
    fn setpgid_creating_new_group_from_child() {
        let (system, mut executor) = virtual_system_with_executor();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let child = env.system.new_child_process().unwrap();
        let pid = child(
            &mut env,
            Box::new(|child_env| {
                Box::pin(async move {
                    let result = child_env.system.setpgid(Pid(0), Pid(0));
                    assert_eq!(result, Ok(()));
                    child_env.system.exit(child_env.exit_status).await
                })
            }),
        );
        executor.run_until_stalled();

        let pgid = state.borrow().processes[&pid].pgid();
        assert_eq!(pgid, pid);
    }

    #[test]
    fn setpgid_extending_existing_group_from_parent() {
        let (system, _executor) = virtual_system_with_executor();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let child_1 = env.system.new_child_process().unwrap();
        let pid_1 = child_1(&mut env, Box::new(|_env| Box::pin(pending())));
        env.system.setpgid(pid_1, pid_1).unwrap();
        let child_2 = env.system.new_child_process().unwrap();
        let pid_2 = child_2(&mut env, Box::new(|_env| Box::pin(pending())));

        let result = env.system.setpgid(pid_2, pid_1);
        assert_eq!(result, Ok(()));

        let pgid = state.borrow().processes[&pid_2].pgid();
        assert_eq!(pgid, pid_1);
    }

    #[test]
    fn setpgid_with_nonexisting_pid() {
        let (system, _executor) = virtual_system_with_executor();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let child = env.system.new_child_process().unwrap();
        let pid = child(&mut env, Box::new(|_env| Box::pin(pending())));

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
        let mut env = Env::with_system(system);
        let child = env.system.new_child_process().unwrap();
        let _pid = child(
            &mut env,
            Box::new(move |child_env| {
                Box::pin(async move {
                    let result = child_env.system.setpgid(parent_pid, Pid(0));
                    assert_eq!(result, Err(Errno::ESRCH));
                    child_env.system.exit(child_env.exit_status).await
                })
            }),
        );
        executor.run_until_stalled();

        let pgid = state.borrow().processes[&parent_pid].pgid();
        assert_eq!(pgid, Pid(1));
    }

    #[test]
    fn setpgid_with_execed_child() {
        let (system, mut executor) = virtual_system_with_executor();
        let path = "/some/file";
        let mut content = Inode::default();
        content.body = FileBody::Regular {
            content: vec![],
            is_native_executable: true,
        };
        content.permissions.set(Mode::USER_EXEC, true);
        let content = Rc::new(RefCell::new(content));
        let state = Rc::clone(&system.state);
        state.borrow_mut().file_system.save(path, content).unwrap();
        let mut env = Env::with_system(system);
        let child = env.system.new_child_process().unwrap();
        let pid = child(
            &mut env,
            Box::new(move |child_env| {
                Box::pin(async move {
                    let path = CString::new(path).unwrap();
                    child_env.system.execve(&path, &[], &[]).await.ok();
                    child_env.system.exit(child_env.exit_status).await
                })
            }),
        );
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
        let mut env = Env::with_system(system);
        let child_1 = env.system.new_child_process().unwrap();
        let pid_1 = child_1(&mut env, Box::new(|_env| Box::pin(pending())));
        // env.system.setpgid(pid_1, pid_1).unwrap();
        let child_2 = env.system.new_child_process().unwrap();
        let pid_2 = child_2(&mut env, Box::new(|_env| Box::pin(pending())));
        executor.run_until_stalled();

        let result = env.system.setpgid(pid_2, pid_1);
        assert_eq!(result, Err(Errno::EPERM));

        let pgid = state.borrow().processes[&pid_2].pgid();
        assert_eq!(pgid, Pid(1));
    }

    #[test]
    fn tcsetpgrp_success() {
        let system = VirtualSystem::new();
        let pid = Pid(10);
        let ppid = system.process_id;
        let pgid = Pid(9);
        system
            .state
            .borrow_mut()
            .processes
            .insert(pid, Process::with_parent_and_group(ppid, pgid));

        system
            .tcsetpgrp(Fd::STDIN, pgid)
            .now_or_never()
            .unwrap()
            .unwrap();

        let foreground = system.state.borrow().foreground;
        assert_eq!(foreground, Some(pgid));
    }

    #[test]
    fn tcsetpgrp_with_invalid_fd() {
        let system = VirtualSystem::new();
        let result = system.tcsetpgrp(Fd(100), Pid(2)).now_or_never().unwrap();
        assert_eq!(result, Err(Errno::EBADF));
    }

    #[test]
    fn tcsetpgrp_with_nonexisting_pgrp() {
        let system = VirtualSystem::new();
        let result = system
            .tcsetpgrp(Fd::STDIN, Pid(100))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Err(Errno::EPERM));
    }

    #[test]
    fn new_child_process_without_executor() {
        let system = VirtualSystem::new();
        let result = system.new_child_process();
        match result {
            Ok(_) => panic!("unexpected Ok value"),
            Err(e) => assert_eq!(e, Errno::ENOSYS),
        }
    }

    #[test]
    fn new_child_process_with_executor() {
        let (system, _executor) = virtual_system_with_executor();

        let result = system.new_child_process();

        let state = system.state.borrow();
        assert_eq!(state.processes.len(), 2);
        drop(state);

        let mut env = Env::with_system(system);
        let child_process = result.unwrap();
        let pid = child_process(
            &mut env,
            Box::new(|env| Box::pin(async move { env.system.exit(env.exit_status).await })),
        );
        assert_eq!(pid, Pid(3));
    }

    #[test]
    fn wait_for_running_child() {
        let (system, _executor) = virtual_system_with_executor();

        let child_process = system.new_child_process();

        let mut env = Env::with_system(system);
        let child_process = child_process.unwrap();
        let pid = child_process(
            &mut env,
            Box::new(|_env| {
                Box::pin(async {
                    unreachable!("child process does not progress unless executor is used")
                })
            }),
        );

        let result = env.system.wait(pid);
        assert_eq!(result, Ok(None))
    }

    #[test]
    fn wait_for_exited_child() {
        let (system, mut executor) = virtual_system_with_executor();

        let child_process = system.new_child_process();

        let mut env = Env::with_system(system);
        let child_process = child_process.unwrap();
        let pid = child_process(
            &mut env,
            Box::new(|env| Box::pin(async move { env.system.exit(ExitStatus(5)).await })),
        );
        executor.run_until_stalled();

        let result = env.system.wait(pid);
        assert_eq!(result, Ok(Some((pid, ProcessState::exited(5)))));
    }

    #[test]
    fn wait_for_signaled_child() {
        let (system, mut executor) = virtual_system_with_executor();

        let child_process = system.new_child_process();

        let mut env = Env::with_system(system);
        let child_process = child_process.unwrap();
        let pid = child_process(
            &mut env,
            Box::new(|env| {
                Box::pin(async move {
                    let pid = env.system.getpid();
                    let result = env.system.kill(pid, Some(SIGKILL)).await;
                    unreachable!("kill returned {result:?}");
                })
            }),
        );
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
        let (system, mut executor) = virtual_system_with_executor();

        let child_process = system.new_child_process();

        let mut env = Env::with_system(system);
        let child_process = child_process.unwrap();
        let pid = child_process(
            &mut env,
            Box::new(|env| {
                Box::pin(async move {
                    let pid = env.system.getpid();
                    let result = env.system.kill(pid, Some(SIGSTOP)).await;
                    unreachable!("kill returned {result:?}");
                })
            }),
        );
        executor.run_until_stalled();

        let result = env.system.wait(pid);
        assert_eq!(result, Ok(Some((pid, ProcessState::stopped(SIGSTOP)))));
    }

    #[test]
    fn wait_for_resumed_child() {
        let (system, mut executor) = virtual_system_with_executor();

        let child_process = system.new_child_process();

        let mut env = Env::with_system(system);
        let child_process = child_process.unwrap();
        let pid = child_process(
            &mut env,
            Box::new(|env| {
                Box::pin(async move {
                    let pid = env.system.getpid();
                    let result = env.system.kill(pid, Some(SIGSTOP)).await;
                    assert_eq!(result, Ok(()));
                    env.system.exit(ExitStatus(123)).await
                })
            }),
        );
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
        let system = VirtualSystem::new();
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
        let (system, mut executor) = virtual_system_with_executor();
        system.sigaction(SIGCHLD, Disposition::Catch).unwrap();

        let child_process = system.new_child_process().unwrap();

        let mut env = Env::with_system(system);
        let _pid = child_process(
            &mut env,
            Box::new(|env| Box::pin(async { env.system.exit(ExitStatus(0)).await })),
        );
        executor.run_until_stalled();

        assert_eq!(env.system.caught_signals(), [SIGCHLD]);
    }

    #[test]
    fn execve_returns_enosys_for_executable_file() {
        let system = VirtualSystem::new();
        let path = "/some/file";
        let mut content = Inode::default();
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
        let result = system.execve(&path, &[], &[]).now_or_never().unwrap();
        assert_eq!(result, Err(Errno::ENOSYS));
    }

    #[test]
    fn execve_saves_arguments() {
        let system = VirtualSystem::new();
        let path = "/some/file";
        let mut content = Inode::default();
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
        system.execve(&path, &args, &envs).now_or_never();

        let process = system.current_process();
        let arguments = process.last_exec.as_ref().unwrap();
        assert_eq!(arguments.0, path);
        assert_eq!(arguments.1, args);
        assert_eq!(arguments.2, envs);
    }

    #[test]
    fn execve_returns_enoexec_for_non_executable_file() {
        let system = VirtualSystem::new();
        let path = "/some/file";
        let mut content = Inode::default();
        content.permissions.set(Mode::USER_EXEC, true);
        let content = Rc::new(RefCell::new(content));
        let mut state = system.state.borrow_mut();
        state.file_system.save(path, content).unwrap();
        drop(state);
        let path = CString::new(path).unwrap();
        let result = system.execve(&path, &[], &[]).now_or_never().unwrap();
        assert_eq!(result, Err(Errno::ENOEXEC));
    }

    #[test]
    fn execve_returns_enoent_on_file_not_found() {
        let system = VirtualSystem::new();
        let result = system
            .execve(c"/no/such/file", &[], &[])
            .now_or_never()
            .unwrap();
        assert_eq!(result, Err(Errno::ENOENT));
    }

    #[test]
    fn exit_sets_current_process_state_to_exited() {
        let system = VirtualSystem::new();
        system.exit(ExitStatus(42)).now_or_never();

        assert!(system.current_process().state_has_changed());
        assert_eq!(
            system.current_process().state(),
            ProcessState::exited(ExitStatus(42))
        );
    }

    #[test]
    fn exit_sends_sigchld_to_parent() {
        let (system, mut executor) = virtual_system_with_executor();
        system.sigaction(SIGCHLD, Disposition::Catch).unwrap();

        let child_process = system.new_child_process().unwrap();

        let mut env = Env::with_system(system);
        let _pid = child_process(
            &mut env,
            Box::new(|env| Box::pin(async { env.system.exit(ExitStatus(123)).await })),
        );
        executor.run_until_stalled();

        assert_eq!(env.system.caught_signals(), [SIGCHLD]);
    }

    #[test]
    fn chdir_changes_directory() {
        let system = VirtualSystem::new();

        // Create a regular file and its parent directory
        let _ = system
            .open(
                c"/dir/file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();

        let result = system.chdir(c"/dir");
        assert_eq!(result, Ok(()));
        assert_eq!(system.current_process().cwd, Path::new("/dir"));
    }

    #[test]
    fn chdir_fails_with_non_existing_directory() {
        let system = VirtualSystem::new();

        let result = system.chdir(c"/no/such/dir");
        assert_eq!(result, Err(Errno::ENOENT));
    }

    #[test]
    fn chdir_fails_with_non_directory_file() {
        let system = VirtualSystem::new();

        // Create a regular file and its parent directory
        let _ = system
            .open(
                c"/dir/file",
                OfdAccess::WriteOnly,
                OpenFlag::Create.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap();

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
                soft: INFINITY,
                hard: INFINITY,
            },
        );
    }

    #[test]
    fn setrlimit_and_getrlimit_with_finite_limits() {
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
        let result = system.setrlimit(Resource::CPU, LimitPair { soft: 2, hard: 1 });
        assert_eq!(result, Err(Errno::EINVAL));

        // The limits should not have been changed
        let result = system.getrlimit(Resource::CPU).unwrap();
        assert_eq!(
            result,
            LimitPair {
                soft: INFINITY,
                hard: INFINITY,
            },
        );
    }

    #[test]
    fn setrlimit_refuses_raising_hard_limit() {
        let system = VirtualSystem::new();
        system
            .setrlimit(Resource::CPU, LimitPair { soft: 1, hard: 1 })
            .unwrap();
        let result = system.setrlimit(Resource::CPU, LimitPair { soft: 1, hard: 2 });
        assert_eq!(result, Err(Errno::EPERM));

        // The limits should not have been changed
        let result = system.getrlimit(Resource::CPU).unwrap();
        assert_eq!(result, LimitPair { soft: 1, hard: 1 });
    }
}

#[cfg(test)]
mod fifo_tests;
