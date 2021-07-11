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
use crate::System;
use nix::errno::Errno;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use std::cell::Ref;
use std::cell::RefCell;
use std::cell::RefMut;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;
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
use std::task::Waker;

// TODO VirtualSystem is not PartialEq because ForkResult is not.
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
        state.processes.insert(process_id, Process::new());

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
    fn clone_box(&self) -> Box<dyn System> {
        Box::new(self.clone())
    }

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

    /// Simulates cloning the current shell process.
    ///
    /// This implementation pops the first entry from
    /// [`pending_forks`](SystemState::pending_forks) and returns it.
    /// If `pending_forks` is empty, this function will **panic**!
    ///
    /// # Safety
    ///
    /// Though [`System::fork`] is declared `unsafe`, this implementation of
    /// `fork` is safe.
    unsafe fn fork(&mut self) -> nix::Result<nix::unistd::ForkResult> {
        self.state
            .borrow_mut()
            .pending_forks
            .pop_front()
            .expect("pending_forks must be filled before calling fork")
    }

    /// Simulates awaiting child process status update.
    ///
    /// This implementation pops the first entry from
    /// [`pending_waits`](Process::pending_waits) and returns it.
    /// If `pending_waits` is empty, this function will **panic**!
    fn wait(&mut self) -> nix::Result<nix::sys::wait::WaitStatus> {
        self.current_process_mut()
            .pending_waits
            .pop_front()
            .expect("pending_waits must be filled before calling wait")
    }

    /// **Panic!**
    ///
    /// The `execve` system call cannot be simulated in the userland. This
    /// function panics if the file at `path` is a native executable. Otherwise,
    /// it returns an error.
    fn execve(
        &mut self,
        path: &CStr,
        args: &[CString],
        _envs: &[CString],
    ) -> nix::Result<Infallible> {
        let path = OsStr::from_bytes(path.to_bytes()).as_ref();
        let fs = &self.state.borrow().file_system;
        if let Some(file) = fs.get(path) {
            // TODO Check file permissions
            if file.is_native_executable {
                panic!(
                    "VirtualSystem::execve called for path={:?}, args={:?}",
                    path, args
                );
            } else {
                Err(Errno::ENOEXEC.into())
            }
        } else {
            // TODO Maybe ENOTDIR
            Err(Errno::ENOENT.into())
        }
    }
}

// TODO SystemState is not Eq because ForkResult is not.
/// State of the virtual system.
#[derive(Clone, Debug, Default)]
pub struct SystemState {
    /// Task manager that can execute asynchronous tasks.
    ///
    /// The virtual system uses this executor to run (virtual) child processes.
    /// If `executor` is `None`, the `fork` function will fail.
    pub executor: Option<Rc<dyn Executor>>,

    /// Processes running in the system.
    pub processes: BTreeMap<Pid, Process>,

    /// Collection of files existing in the virtual system.
    pub file_system: FileSystem,

    /// Results of future calls to [`fork`](VirtualSystem::fork).
    pub pending_forks: VecDeque<nix::Result<nix::unistd::ForkResult>>,
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
#[derive(Clone, Debug, Default)]
pub struct Process {
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

    /// Results of future calls to [`wait`](VirtualSystem::wait).
    pub pending_waits: VecDeque<nix::Result<nix::sys::wait::WaitStatus>>,
}

impl Process {
    /// Creates a new running process.
    pub fn new() -> Process {
        Process::default()
    }

    /// Returns the process state.
    pub fn state(&self) -> ProcessState {
        self.state
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

impl Default for ProcessState {
    /// Returns `Running`.
    fn default() -> ProcessState {
        ProcessState::Running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    #[should_panic(expected = r#"VirtualSystem::execve called for path="/some/file", args=[]"#)]
    fn execve_panics_for_executable_file() {
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
        unreachable!("{:?}", result);
    }

    #[test]
    fn execve_returns_for_non_executable_file() {
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
    fn execve_returns_on_file_not_found() {
        let mut system = VirtualSystem::new();
        let path = CString::new("/no/such/file").unwrap();
        let result = system.execve(&path, &[], &[]);
        assert_eq!(result, Err(Errno::ENOENT.into()));
    }
}
