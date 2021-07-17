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

use crate::exec::ExitStatus;
use crate::ChildProcess;
use crate::Env;
use crate::System;
use async_trait::async_trait;
use nix::errno::Errno;
use nix::sys::signal::Signal;
use nix::sys::wait::WaitStatus;
use nix::unistd::Pid;
use std::cell::Ref;
use std::cell::RefCell;
use std::cell::RefMut;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::convert::Infallible;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::future::Future;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
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
    /// process ID 2 in the process set ([`SystemState::processes`]). Other
    /// members of `SystemState` will be empty.
    pub fn new() -> VirtualSystem {
        let mut state = SystemState::default();
        let process_id = Pid::from_raw(2);
        let process = Process::with_parent(Pid::from_raw(1));
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
            Some(inode) => inode.permissions.0 & 0o111 != 0,
        }
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
        let executor = state
            .executor
            .clone()
            .ok_or(nix::Error::Sys(Errno::ENOSYS))?;
        let process_id = state
            .processes
            .keys()
            .max()
            .map_or(Pid::from_raw(2), |pid| Pid::from_raw(pid.as_raw() + 1));
        let child_process = Process::with_parent(self.process_id);
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
                return Poll::Ready(Err(Errno::ECHILD.into()));
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
            if file.is_native_executable {
                // Save arguments in the Process
                let process = state.processes.get_mut(&self.process_id).unwrap();
                let path = path.to_owned();
                let args = args.to_owned();
                let envs = envs.to_owned();
                process.last_exec = Some((path, args, envs));

                Err(Errno::ENOSYS.into())
            } else {
                Err(Errno::ENOEXEC.into())
            }
        } else {
            // TODO Maybe ENOTDIR
            Err(Errno::ENOENT.into())
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

/// Collection of files.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileSystem(HashMap<PathBuf, INode>);
// TODO should be a link to the root i-node
// In the current implementation, this hash map stores all files in a flat
// namespace, without any recursive directory structure.

impl FileSystem {
    /// Saves a file.
    ///
    /// If there is an existing file at the specified path, it is replaced with
    /// the new file and returned.
    pub fn save(&mut self, path: PathBuf, content: INode) -> Option<INode> {
        self.0.insert(path, content)
    }

    /// Returns a reference to the existing file at the specified path.
    pub fn get(&self, path: &Path) -> Option<&INode> {
        // TODO Return ENOTDIR or ENOENT if not found
        self.0.get(path)
    }
}

/// File on the file system.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct INode {
    /// Access permissions.
    pub permissions: Mode,
    /// Whether this file is a native binary that can be exec'ed.
    pub is_native_executable: bool,
    // TODO File content, owner user and group, etc.
}

impl INode {
    /// Create an empty regular file.
    pub fn new() -> INode {
        INode::default()
    }
}

/// File permission bits.
///
/// The `Default` mode is `0o644`, not `0o000`.
#[derive(Copy, Clone, Eq, Hash, PartialEq)]
pub struct Mode(pub u32);

impl Debug for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mode({:#o})", self.0)
    }
}

impl Default for Mode {
    fn default() -> Mode {
        Mode(0o644)
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

/// Process in a virtual system.
#[derive(Clone, Debug)]
pub struct Process {
    /// Process ID of the parent process.
    ppid: Pid,

    /// State of the process.
    state: ProcessState,

    /// References to tasks that are waiting for the process state to change.
    ///
    /// If this is `None`, the `state` has changed but not yet been reported by
    /// the `wait` system call. The next `wait` call should immediately notify
    /// the current state. If this is `Some(_)`, the `state` has not changed
    /// since the last `wait` call. The next `wait` call should leave a waker
    /// so that the caller is woken when the state changes later.
    state_awaiters: Option<Vec<Waker>>,

    /// Copy of arguments passed to [`execve`](VirtualSystem::execve).
    last_exec: Option<(CString, Vec<CString>, Vec<CString>)>,
}

impl Process {
    /// Creates a new running process.
    pub fn with_parent(ppid: Pid) -> Process {
        Process {
            ppid,
            state: ProcessState::Running,
            state_awaiters: Some(Vec::new()),
            last_exec: None,
        }
    }

    /// Returns the process ID of the parent process.
    pub fn ppid(&self) -> Pid {
        self.ppid
    }

    /// Returns the process state.
    pub fn state(&self) -> ProcessState {
        self.state
    }

    /// Sets the state of this process.
    ///
    /// This function returns wakers that must be woken. The caller must first
    /// drop the `RefMut` borrowing the [`SystemState`] containing this
    /// `Process` and then wake the wakers returned from this function. This is
    /// to prevent a possible second borrow by another task.
    #[must_use]
    pub fn set_state(&mut self, state: ProcessState) -> Vec<Waker> {
        let old_state = std::mem::replace(&mut self.state, state);

        if old_state == state {
            Vec::new()
        } else {
            self.state_awaiters.take().unwrap_or_else(Vec::new)
        }
    }

    /// Returns the arguments to the last call to
    /// [`execve`](VirtualSystem::execve) on this process.
    #[must_use]
    pub fn last_exec(&self) -> &Option<(CString, Vec<CString>, Vec<CString>)> {
        &self.last_exec
    }
}

/// State of a process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    Running,
    Stopped(Signal),
    Exited(ExitStatus),
    Signaled(Signal),
}

impl ProcessState {
    /// Converts `ProcessState` to `WaitStatus`.
    #[must_use]
    pub fn to_wait_status(self, pid: Pid) -> WaitStatus {
        match self {
            ProcessState::Running => WaitStatus::Continued(pid),
            ProcessState::Exited(exit_status) => WaitStatus::Exited(pid, exit_status.0),
            ProcessState::Stopped(signal) => WaitStatus::Stopped(pid, signal),
            ProcessState::Signaled(signal) => WaitStatus::Signaled(pid, signal, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let content = INode::default();
        system.state.borrow_mut().file_system.save(path, content);
        assert!(!system.is_executable_file(&CString::new("/some/file").unwrap()));
    }

    #[test]
    fn is_executable_file_with_executable_file() {
        let system = VirtualSystem::new();
        let path = PathBuf::from("/some/file");
        let mut content = INode::default();
        content.permissions.0 |= 0o100;
        system.state.borrow_mut().file_system.save(path, content);
        assert!(system.is_executable_file(&CString::new("/some/file").unwrap()));
    }

    #[test]
    fn new_child_process_without_executor() {
        let mut system = VirtualSystem::new();
        let result = unsafe { system.new_child_process() };
        assert_eq!(result.unwrap_err(), Errno::ENOSYS.into());
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
        assert_eq!(result, Err(Errno::ECHILD.into()));
    }

    #[test]
    fn execve_returns_enosys_for_executable_file() {
        let mut system = VirtualSystem::new();
        let path = PathBuf::from("/some/file");
        let mut content = INode::default();
        content.permissions.0 |= 0o100;
        content.is_native_executable = true;
        system
            .state
            .borrow_mut()
            .file_system
            .save(path.clone(), content);
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        let result = system.execve(&path, &[], &[]);
        assert_eq!(result, Err(Errno::ENOSYS.into()));
    }

    #[test]
    fn execve_saves_arguments() {
        let mut system = VirtualSystem::new();
        let path = PathBuf::from("/some/file");
        let mut content = INode::default();
        content.permissions.0 |= 0o100;
        content.is_native_executable = true;
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
        system
            .state
            .borrow_mut()
            .file_system
            .save(path.clone(), content);
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        let result = system.execve(&path, &[], &[]);
        assert_eq!(result, Err(Errno::ENOEXEC.into()));
    }

    #[test]
    fn execve_returns_enoent_on_file_not_found() {
        let mut system = VirtualSystem::new();
        let path = CString::new("/no/such/file").unwrap();
        let result = system.execve(&path, &[], &[]);
        assert_eq!(result, Err(Errno::ENOENT.into()));
    }
}
