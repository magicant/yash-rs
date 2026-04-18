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

//! Utilities for performing tests that interact with the shell environment
//!
//! This module is conditionally compiled when the `test-helper` feature is enabled.

use crate::Env;
use crate::system::r#virtual::{Executor, FileBody, Inode, SystemState, VirtualSystem};
use assert_matches::assert_matches;
use futures_executor::LocalSpawner;
use futures_util::FutureExt as _;
use futures_util::task::LocalSpawnExt as _;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::str::from_utf8;

/// Adapter for [`LocalSpawner`] to [`Executor`]
#[derive(Clone, Debug)]
pub struct LocalExecutor(pub LocalSpawner);

impl Executor for LocalExecutor {
    fn spawn(
        &self,
        task: Pin<Box<dyn Future<Output = ()>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(self.0.spawn_local(task)?)
    }
}

/// Allows `Spawner` to be used as an `Executor` in the virtual system.
///
/// Remember that `yash_executor::Spawner` is for single-threaded processes.
/// It is not safe to use it in a multi-threaded context, e.g. by spawning a
/// task that creates threads and uses wakers from the executor in those
/// threads.
impl<'a> Executor for yash_executor::Spawner<'a> {
    fn spawn(
        &self,
        task: Pin<Box<dyn Future<Output = ()>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // SAFETY: Actually this is not safe if the task creates a thread and
        // a waker from the executor is used in the thread. However, the shell
        // process must be single-threaded to work correctly, so we assume the
        // task does not create threads.
        (unsafe { self.spawn_pinned(task) })
            .map_err(|_| "failed to spawn task: the executor has been dropped".into())
    }
}

/// Runs an asynchronous function in a virtual system with a local executor.
///
/// This function creates a [`VirtualSystem`] and installs a [`LocalExecutor`]
/// in it. The argument function `f` is called with an [`Env`] with the virtual
/// system. The executor is run until the task returned by `f` completes, and
/// the result is returned.
///
/// Function `f` is called with two arguments: the [`Env`] and a shared
/// reference to the system state. The system state can be used to interact
/// with the virtual system, e.g. to create files or concurrent tasks.
///
/// This function is useful for testing asynchronous code that spawns tasks
/// that need to be run concurrently with the main task.
pub fn in_virtual_system<F, Fut, T>(f: F) -> T
where
    F: FnOnce(Env<VirtualSystem>, Rc<RefCell<SystemState>>) -> Fut,
    Fut: Future<Output = T> + 'static,
    T: 'static,
{
    let system = VirtualSystem::new();
    let state = Rc::clone(&system.state);
    let global_executor = yash_executor::Executor::new();
    state.borrow_mut().executor = Some(Rc::new(global_executor.spawner()));

    let env = Env::with_system(system);
    let selector = env.system.clone();
    let task = f(env, Rc::clone(&state));

    // Wrap the task in a loop that performs `select` to drive the executor
    // until the task completes. This simulates the main loop of the shell
    // process.
    let task = async move {
        let local_executor = yash_executor::Executor::new();
        // SAFETY: Actually this is not safe if the task creates a thread and
        // a waker from the executor is used in the thread. However, the shell
        // process must be single-threaded to work correctly, so we assume the
        // task does not create threads.
        let mut result_future = unsafe { local_executor.spawn(task) };
        loop {
            local_executor.run_until_stalled();
            if let Some(result) = (&mut result_future).now_or_never() {
                return result;
            }
            selector.select_async().await.ok();
        }
    };

    // SAFETY: The same as above.
    let mut task = unsafe { global_executor.spawn(task) };

    // The outer, global executor allows the task to spawn child processes that
    // are run concurrently with the main task.
    loop {
        global_executor.run_until_stalled();
        if let Some(result) = (&mut task).now_or_never() {
            return result;
        }

        let mut state = state.borrow_mut();
        if let Some(next_wake_time) = state.scheduled_wakers.next_wake_time() {
            state.advance_time(next_wake_time);
        }
    }
}

/// Creates a dummy file at /dev/tty.
pub fn stub_tty(state: &RefCell<SystemState>) {
    state
        .borrow_mut()
        .file_system
        .save("/dev/tty", Rc::new(RefCell::new(Inode::new([]))))
        .unwrap();
}

/// Helper function for asserting on the content of /dev/stdout
///
/// This function asserts on the content of /dev/stdout. The argument function
/// `f` is called with the content of /dev/stdout as a string slice.
///
/// This function panics if /dev/stdout does not exist, is not a regular file,
/// or does not contain a valid UTF-8 string.
///
/// # Example
///
/// ```
/// # use std::rc::Rc;
/// # use futures_util::FutureExt as _;
/// # use yash_env::Env;
/// # use yash_env::io::Fd;
/// # use yash_env::system::Write as _;
/// # use yash_env::system::r#virtual::VirtualSystem;
/// # use yash_env::test_helper::assert_stdout;
/// # async fn f() {
/// let system = VirtualSystem::new();
/// let state = Rc::clone(&system.state);
/// let mut env = Env::with_system(system);
/// env.system.write(Fd::STDOUT, b"Hello, world!\n").await.unwrap();
/// assert_stdout(&state, |stdout| assert_eq!(stdout, "Hello, world!\n"));
/// # }
/// # f().now_or_never().unwrap();
/// ```
pub fn assert_stdout<F, T>(state: &RefCell<SystemState>, f: F) -> T
where
    F: FnOnce(&str) -> T,
{
    let stdout = state.borrow().file_system.get("/dev/stdout").unwrap();
    let stdout = stdout.borrow();
    assert_matches!(&stdout.body, FileBody::Regular { content, .. } => {
        f(from_utf8(content).unwrap())
    })
}

/// Helper function for asserting on the content of /dev/stderr
///
/// This function asserts on the content of /dev/stderr. The argument function
/// `f` is called with the content of /dev/stderr as a string slice.
///
/// This function panics if /dev/stderr does not exist, is not a regular file,
/// or does not contain a valid UTF-8 string.
///
/// This function is analogous to [`assert_stdout`]. See its documentation for
/// an example.
pub fn assert_stderr<F, T>(state: &RefCell<SystemState>, f: F) -> T
where
    F: FnOnce(&str) -> T,
{
    let stderr = state.borrow().file_system.get("/dev/stderr").unwrap();
    let stderr = stderr.borrow();
    assert_matches!(&stderr.body, FileBody::Regular { content, .. } => {
        f(from_utf8(content).unwrap())
    })
}

pub mod function;
mod wake_flag;

pub use wake_flag::WakeFlag;
