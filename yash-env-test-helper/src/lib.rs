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

//! This crate contains utility functions for use in tests that interact with
//! the shell environment ([`yash_env::Env`]).

use assert_matches::assert_matches;
use futures_executor::LocalSpawner;
use futures_util::FutureExt as _;
use futures_util::task::LocalSpawnExt as _;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::str::from_utf8;
use yash_env::Env;
use yash_env::system::r#virtual::{Executor, FileBody, Inode, SystemState, VirtualSystem};

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
    let mut executor = futures_executor::LocalPool::new();
    state.borrow_mut().executor = Some(Rc::new(LocalExecutor(executor.spawner())));

    let env = Env::with_system(system);
    let shared_system = env.system.clone();
    let task = f(env, Rc::clone(&state));
    let mut task = executor.spawner().spawn_local_with_handle(task).unwrap();
    loop {
        if let Some(result) = (&mut task).now_or_never() {
            return result;
        }
        executor.run_until_stalled();
        shared_system.select(false).unwrap();
        SystemState::select_all(&state);
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
/// # use yash_env::Env;
/// # use yash_env::io::Fd;
/// # use yash_env::system::System;
/// # use yash_env::system::r#virtual::VirtualSystem;
/// # use yash_env_test_helper::assert_stdout;
/// let system = VirtualSystem::new();
/// let state = Rc::clone(&system.state);
/// let mut env = Env::with_system(system);
/// env.system.write(Fd::STDOUT, b"Hello, world!\n").unwrap();
/// assert_stdout(&state, |stdout| assert_eq!(stdout, "Hello, world!\n"));
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
