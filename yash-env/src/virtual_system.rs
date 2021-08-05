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
//! Currently, only regular files are supported in virtual systems.
//!
//! Pathname resolution is not yet fully simulated. Currently, files are naively
//! identified by their full path.
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

mod file_system;
mod io;
mod process;

pub use self::file_system::*;
pub use self::io::*;
pub use self::process::*;
use crate::io::Fd;
use crate::ChildProcess;
use crate::Env;
use crate::System;
use async_trait::async_trait;
use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::sys::select::FdSet;
use nix::sys::wait::WaitStatus;
use nix::unistd::Pid;
use std::cell::Ref;
use std::cell::RefCell;
use std::cell::RefMut;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::convert::TryInto;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::future::Future;
use std::os::raw::c_int;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Poll;
use std::task::Waker;

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
    /// and 2, respectively.
    pub fn new() -> VirtualSystem {
        let mut state = SystemState::default();
        let mut process = Process::with_parent(Pid::from_raw(1));
        let mut set_std_fd = |path, fd| {
            let file_system = &mut state.file_system;
            let file = Rc::new(RefCell::new(INode::new()));
            file_system.save(PathBuf::from(path), Rc::clone(&file));
            let body = FdBody {
                open_file_description: Rc::new(RefCell::new(OpenFile {
                    file,
                    offset: 0,
                    is_readable: true,
                    is_writable: true,
                })),
                cloexec: false,
            };
            process.set_fd(fd, body).unwrap();
        };
        set_std_fd("/dev/stdin", Fd::STDIN);
        set_std_fd("/dev/stdout", Fd::STDOUT);
        set_std_fd("/dev/stderr", Fd::STDERR);

        let process_id = Pid::from_raw(2);
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
    pub fn with_open_file_description<F, R>(&mut self, fd: Fd, f: F) -> nix::Result<R>
    where
        F: FnOnce(&mut dyn OpenFileDescription) -> nix::Result<R>,
    {
        let mut process = self.current_process_mut();
        let body = process.get_fd_mut(fd).ok_or(Errno::EBADF)?;
        let mut ofd = body.open_file_description.borrow_mut();
        use std::ops::DerefMut;
        f(ofd.deref_mut())
    }
}

impl Default for VirtualSystem {
    fn default() -> Self {
        VirtualSystem::new()
    }
}

impl System for VirtualSystem {
    /// Tests whether the specified file is executable or not.
    ///
    /// The current implementation only checks if the file has any executable
    /// bit in the permissions. The file owner and group are not considered.
    fn is_executable_file(&self, path: &CStr) -> bool {
        let path = match path.to_str() {
            Ok(path) => PathBuf::from(path),
            Err(_) => return false,
        };
        match self.state.borrow().file_system.get(&path) {
            None => false,
            Some(inode) => inode.borrow().permissions.0 & 0o111 != 0,
        }
    }

    fn pipe(&mut self) -> nix::Result<(Fd, Fd)> {
        let pipe = Rc::new(RefCell::new(Pipe::new()));
        let writer = Rc::new(RefCell::new(PipeWriter {
            pipe: Rc::downgrade(&pipe),
        }));
        let reader = Rc::new(RefCell::new(PipeReader { pipe }));

        let reader = FdBody {
            open_file_description: reader,
            cloexec: false,
        };
        let writer = FdBody {
            open_file_description: writer,
            cloexec: false,
        };

        let mut process = self.current_process_mut();
        let reader = process.open_fd(reader).map_err(|_| Errno::EMFILE)?;
        let writer = process.open_fd(writer).map_err(|_| {
            process.close_fd(reader);
            Errno::EMFILE
        })?;
        Ok((reader, writer))
    }

    fn dup(&mut self, from: Fd, to_min: Fd, cloexec: bool) -> nix::Result<Fd> {
        let mut process = self.current_process_mut();
        let mut body = process.fds.get(&from).ok_or(Errno::EBADF)?.clone();
        body.cloexec = cloexec;
        process.open_fd_ge(to_min, body).map_err(|_| Errno::EMFILE)
    }

    fn dup2(&mut self, from: Fd, to: Fd) -> nix::Result<Fd> {
        let mut process = self.current_process_mut();
        let mut body = process.fds.get(&from).ok_or(Errno::EBADF)?.clone();
        body.cloexec = false;
        process.set_fd(to, body).map_err(|_| Errno::EBADF)?;
        Ok(to)
    }

    fn close(&mut self, fd: Fd) -> nix::Result<()> {
        self.current_process_mut().close_fd(fd);
        Ok(())
    }

    /// Current implementation returns `Ok(OFlag::empty())`.
    fn fcntl_getfl(&self, _fd: Fd) -> nix::Result<OFlag> {
        // TODO do what this function should do
        Ok(OFlag::empty())
    }

    /// Current implementation does nothing but return `Ok(())`.
    fn fcntl_setfl(&mut self, _fd: Fd, _flags: OFlag) -> nix::Result<()> {
        // TODO do what this function should do
        Ok(())
    }

    fn read(&mut self, fd: Fd, buffer: &mut [u8]) -> nix::Result<usize> {
        self.with_open_file_description(fd, |ofd| ofd.read(buffer))
    }

    fn write(&mut self, fd: Fd, buffer: &[u8]) -> nix::Result<usize> {
        self.with_open_file_description(fd, |ofd| ofd.write(buffer))
    }

    /// Waits for a next event.
    ///
    /// The `VirtualSystem` implementation for this method does not actually
    /// block the calling thread. The method returns immediately in any case.
    fn select(&mut self, readers: &mut FdSet, writers: &mut FdSet) -> nix::Result<c_int> {
        let process = self.current_process();

        for fd in 0..(nix::sys::select::FD_SETSIZE as c_int) {
            if readers.contains(fd) {
                if let Some(body) = process.fds().get(&Fd(fd)) {
                    let ofd = body.open_file_description.borrow();
                    if ofd.is_readable() {
                        if !ofd.is_ready_for_reading() {
                            readers.remove(fd);
                        }
                    } else {
                        return Err(Errno::EBADF);
                    }
                } else {
                    return Err(Errno::EBADF);
                }
            }

            if writers.contains(fd) {
                if let Some(body) = process.fds().get(&Fd(fd)) {
                    let ofd = body.open_file_description.borrow();
                    if ofd.is_writable() {
                        if !ofd.is_ready_for_writing() {
                            writers.remove(fd);
                        }
                    } else {
                        return Err(Errno::EBADF);
                    }
                } else {
                    return Err(Errno::EBADF);
                }
            }
        }

        let reader_count = readers.fds(None).count();
        let writer_count = writers.fds(None).count();
        Ok((reader_count + writer_count).try_into().unwrap())
    }

    /// Creates a new child process.
    ///
    /// This implementation does not create any real child process. Instead,
    /// it returns an implementor of [`ChildProcess`] that `run`s its task
    /// concurrently in the same process.
    ///
    /// To run the concurrent task, this function needs an executor that has
    /// been set in the system state. If the system state does not have an
    /// executor, this function fails with `Errno::ENOSYS`.
    ///
    /// The process ID of the child will be the maximum of existing process IDs
    /// plus 1. If there are no other processes, it will be 2.
    unsafe fn new_child_process(&mut self) -> nix::Result<Box<dyn ChildProcess>> {
        let mut state = self.state.borrow_mut();
        let executor = state.executor.clone().ok_or(Errno::ENOSYS)?;
        let process_id = state
            .processes
            .keys()
            .max()
            .map_or(Pid::from_raw(2), |pid| Pid::from_raw(pid.as_raw() + 1));
        let parent_process = &state.processes[&self.process_id];
        let child_process = Process::fork_from(self.process_id, parent_process);
        state.processes.insert(process_id, child_process);
        drop(state);

        Ok(Box::new(DummyChildProcess {
            state: self.state.clone(),
            executor,
            process_id,
        }))
    }

    /// This function is currently not implemented.
    fn wait(&mut self) -> nix::Result<WaitStatus> {
        todo!()
    }

    /// Reports updated status of a child process.
    ///
    /// This function does not block, but the caller must await the returned
    /// future to obtain the result.
    ///
    /// This function does not remove terminated processes from the system state
    /// so you can examine them later.
    fn wait_sync(&mut self) -> Pin<Box<dyn Future<Output = nix::Result<WaitStatus>> + '_>> {
        Box::pin(futures::future::poll_fn(move |context| {
            let parent_pid = self.process_id;
            let mut state = self.state.borrow_mut();

            // If any child's state has changed, return it
            let mut found_child = false;
            for (pid, process) in &mut state.processes {
                if process.ppid == parent_pid {
                    found_child = true;
                    if process.state_awaiters.is_none() {
                        process.state_awaiters = Some(Vec::new());
                        return Poll::Ready(Ok(process.state.to_wait_status(*pid)));
                    }
                }
            }

            if !found_child {
                return Poll::Ready(Err(Errno::ECHILD));
            }

            // Save a waker so the future is polled again when the state has changed
            for process in state.processes.values_mut() {
                if process.ppid == parent_pid {
                    if let Some(awaiters) = &mut process.state_awaiters {
                        awaiters.push(context.waker().clone());
                    }
                }
            }
            Poll::Pending
        }))
    }

    /// Stub for the `execve` system call.
    ///
    /// The `execve` system call cannot be simulated in the userland. This
    /// function returns `ENOSYS` if the file at `path` is a native executable,
    /// `ENOEXEC` if a non-executable file, and `ENOENT` otherwise.
    fn execve(
        &mut self,
        path: &CStr,
        args: &[CString],
        envs: &[CString],
    ) -> nix::Result<Infallible> {
        let os_path = OsStr::from_bytes(path.to_bytes()).as_ref();
        let mut state = self.state.borrow_mut();
        let fs = &state.file_system;
        if let Some(file) = fs.get(os_path) {
            // TODO Check file permissions
            if file.borrow().is_native_executable {
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
        } else {
            // TODO Maybe ENOTDIR
            Err(Errno::ENOENT)
        }
    }
}

/// Implementor of [`ChildProcess`] that is returned from
/// [`VirtualSystem::new_child_process`].
#[derive(Debug)]
struct DummyChildProcess {
    /// State of the system.
    state: Rc<RefCell<SystemState>>,
    /// Executor to run the child process's task.
    executor: Rc<dyn Executor>,
    /// Process ID of this child process.
    process_id: Pid,
}

#[async_trait(?Send)]
impl ChildProcess for DummyChildProcess {
    async fn run(&mut self, env: &mut Env, mut task: super::ChildProcessTask) -> Pid {
        let state = self.state.clone();
        let process_id = self.process_id;
        let system = VirtualSystem { state, process_id };
        let mut child_env = env.clone_with_system(Box::new(system));

        let state = self.state.clone();
        let run_task_and_set_exit_status = Box::pin(async move {
            task(&mut child_env).await;

            let mut state = state.borrow_mut();
            let process = state
                .processes
                .get_mut(&process_id)
                .expect("the child process is missing");
            let wakers = process.set_state(ProcessState::Exited(child_env.exit_status));
            drop(state);
            wakers.into_iter().for_each(Waker::wake);
        });

        self.executor
            .spawn(run_task_and_set_exit_status)
            .expect("the executor failed to start the child process task");

        process_id
    }
}

/// State of the virtual system.
#[derive(Clone, Debug, Default)]
pub struct SystemState {
    /// Task manager that can execute asynchronous tasks.
    ///
    /// The virtual system uses this executor to run (virtual) child processes.
    /// If `executor` is `None`, [`VirtualSystem::new_child_process`] will fail.
    pub executor: Option<Rc<dyn Executor>>,

    /// Processes running in the system.
    pub processes: BTreeMap<Pid, Process>,

    /// Collection of files existing in the virtual system.
    pub file_system: FileSystem,
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
    ) -> Result<(), Box<dyn std::error::Error>>;
}

impl Executor for futures::executor::LocalSpawner {
    fn spawn(
        &self,
        task: Pin<Box<dyn Future<Output = ()>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use futures::task::LocalSpawnExt;
        self.spawn_local(task)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::ExitStatus;
    use futures::executor::block_on;
    use futures::executor::LocalPool;
    use std::ffi::CString;

    #[test]
    fn is_executable_file_non_existing_file() {
        let system = VirtualSystem::new();
        assert!(!system.is_executable_file(&CString::new("/no/such/file").unwrap()));
    }

    #[test]
    fn is_executable_file_existing_but_non_executable_file() {
        let system = VirtualSystem::new();
        let path = PathBuf::from("/some/file");
        let content = Rc::new(RefCell::new(INode::default()));
        system.state.borrow_mut().file_system.save(path, content);
        assert!(!system.is_executable_file(&CString::new("/some/file").unwrap()));
    }

    #[test]
    fn is_executable_file_with_executable_file() {
        let system = VirtualSystem::new();
        let path = PathBuf::from("/some/file");
        let mut content = INode::default();
        content.permissions.0 |= 0o100;
        let content = Rc::new(RefCell::new(content));
        system.state.borrow_mut().file_system.save(path, content);
        assert!(system.is_executable_file(&CString::new("/some/file").unwrap()));
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
        let result = system.dup(Fd::STDOUT, Fd::STDERR, false);
        assert_eq!(result, Ok(Fd(3)));

        let process = system.current_process();
        let fd1 = process.fds.get(&Fd(1)).unwrap();
        let fd3 = process.fds.get(&Fd(3)).unwrap();
        assert_eq!(fd1, fd3);
    }

    #[test]
    fn dup_can_set_cloexec() {
        let mut system = VirtualSystem::new();
        let result = system.dup(Fd::STDOUT, Fd::STDERR, true);
        assert_eq!(result, Ok(Fd(3)));

        let process = system.current_process();
        let fd3 = process.fds.get(&Fd(3)).unwrap();
        assert!(fd3.cloexec);
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
        process.fds.get_mut(&Fd::STDOUT).unwrap().cloexec = true;
        drop(process);

        let result = system.dup2(Fd::STDOUT, Fd(6));
        assert_eq!(result, Ok(Fd(6)));

        let process = system.current_process();
        let fd6 = process.fds.get(&Fd(6)).unwrap();
        assert!(!fd6.cloexec);
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
    fn select_regular_file_is_always_ready() {
        let mut system = VirtualSystem::new();
        let mut readers = FdSet::new();
        readers.insert(Fd::STDIN.0);
        let mut writers = FdSet::new();
        readers.insert(Fd::STDOUT.0);
        readers.insert(Fd::STDERR.0);

        let all_readers = readers;
        let all_writers = writers;
        let result = system.select(&mut readers, &mut writers);
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
        readers.insert(reader.0);

        let all_readers = readers;
        let all_writers = writers;
        let result = system.select(&mut readers, &mut writers);
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
        readers.insert(reader.0);

        let all_readers = readers;
        let all_writers = writers;
        let result = system.select(&mut readers, &mut writers);
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
        readers.insert(reader.0);

        let result = system.select(&mut readers, &mut writers);
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
        writers.insert(writer.0);

        let all_readers = readers;
        let all_writers = writers;
        let result = system.select(&mut readers, &mut writers);
        assert_eq!(result, Ok(1));
        assert_eq!(readers, all_readers);
        assert_eq!(writers, all_writers);
    }

    #[test]
    fn select_on_unreadable_fd() {
        let mut system = VirtualSystem::new();
        let (_reader, writer) = system.pipe().unwrap();
        let mut fds = FdSet::new();
        fds.insert(writer.0);
        let result = system.select(&mut fds, &mut FdSet::new());
        assert_eq!(result, Err(Errno::EBADF));
    }

    #[test]
    fn select_on_unwritable_fd() {
        let mut system = VirtualSystem::new();
        let (reader, _writer) = system.pipe().unwrap();
        let mut fds = FdSet::new();
        fds.insert(reader.0);
        let result = system.select(&mut FdSet::new(), &mut fds);
        assert_eq!(result, Err(Errno::EBADF));
    }

    #[test]
    fn select_on_closed_fd() {
        let mut system = VirtualSystem::new();
        let mut fds = FdSet::new();
        fds.insert(17);
        let result = system.select(&mut fds, &mut FdSet::new());
        assert_eq!(result, Err(Errno::EBADF));

        let result = system.select(&mut FdSet::new(), &mut fds);
        assert_eq!(result, Err(Errno::EBADF));
    }

    #[test]
    fn new_child_process_without_executor() {
        let mut system = VirtualSystem::new();
        let result = unsafe { system.new_child_process() };
        assert_eq!(result.unwrap_err(), Errno::ENOSYS);
    }

    #[test]
    fn new_child_process_with_executor() {
        let mut system = VirtualSystem::new();
        let mut executor = LocalPool::new();
        let mut state = system.state.borrow_mut();
        state.executor = Some(Rc::new(executor.spawner()));
        drop(state);

        let result = unsafe { system.new_child_process() };

        let state = system.state.borrow();
        assert_eq!(state.processes.len(), 2);
        drop(state);

        let mut env = Env::with_system(Box::new(system));
        let mut child_process = result.unwrap();
        let future = child_process.run(&mut env, Box::new(|_env| Box::pin(async {})));
        let pid = executor.run_until(future);
        assert_eq!(pid, Pid::from_raw(3));
    }

    #[test]
    fn wait_sync_for_exited_process() {
        let mut system = VirtualSystem::new();
        let mut executor = LocalPool::new();
        let mut state = system.state.borrow_mut();
        state.executor = Some(Rc::new(executor.spawner()));
        drop(state);

        let child_process = unsafe { system.new_child_process() };

        let mut env = Env::with_system(Box::new(system));
        let mut child_process = child_process.unwrap();
        let future = child_process.run(
            &mut env,
            Box::new(|env| {
                Box::pin(async move {
                    env.exit_status = ExitStatus(5);
                })
            }),
        );
        let pid = executor.run_until(future);

        #[allow(deprecated)]
        let future = env.system.wait_sync();
        let result = executor.run_until(future);
        assert_eq!(result, Ok(WaitStatus::Exited(pid, 5)))
    }

    #[test]
    fn wait_sync_without_child() {
        let mut system = VirtualSystem::new();
        #[allow(deprecated)]
        let result = block_on(system.wait_sync());
        assert_eq!(result, Err(Errno::ECHILD));
    }

    #[test]
    fn execve_returns_enosys_for_executable_file() {
        let mut system = VirtualSystem::new();
        let path = PathBuf::from("/some/file");
        let mut content = INode::default();
        content.permissions.0 |= 0o100;
        content.is_native_executable = true;
        let content = Rc::new(RefCell::new(content));
        system
            .state
            .borrow_mut()
            .file_system
            .save(path.clone(), content);
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        let result = system.execve(&path, &[], &[]);
        assert_eq!(result, Err(Errno::ENOSYS));
    }

    #[test]
    fn execve_saves_arguments() {
        let mut system = VirtualSystem::new();
        let path = PathBuf::from("/some/file");
        let mut content = INode::default();
        content.permissions.0 |= 0o100;
        content.is_native_executable = true;
        let content = Rc::new(RefCell::new(content));
        system
            .state
            .borrow_mut()
            .file_system
            .save(path.clone(), content);
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        let args = [CString::new("file").unwrap(), CString::new("bar").unwrap()];
        let envs = [
            CString::new("foo=FOO").unwrap(),
            CString::new("baz").unwrap(),
        ];
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
        let path = PathBuf::from("/some/file");
        let mut content = INode::default();
        content.permissions.0 |= 0o100;
        let content = Rc::new(RefCell::new(content));
        system
            .state
            .borrow_mut()
            .file_system
            .save(path.clone(), content);
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        let result = system.execve(&path, &[], &[]);
        assert_eq!(result, Err(Errno::ENOEXEC));
    }

    #[test]
    fn execve_returns_enoent_on_file_not_found() {
        let mut system = VirtualSystem::new();
        let path = CString::new("/no/such/file").unwrap();
        let result = system.execve(&path, &[], &[]);
        assert_eq!(result, Err(Errno::ENOENT));
    }
}
