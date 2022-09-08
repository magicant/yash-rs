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

//! Implementation of the shell built-in utilities.
//!
//! TODO Elaborate

pub mod alias;
pub mod r#break;
pub mod common;
pub mod r#continue;
pub mod jobs;
pub mod readonly;
pub mod r#return;
pub mod set;
pub mod trap;
pub mod wait;

#[doc(no_inline)]
pub use yash_env::builtin::*;

use Type::{Intrinsic, Special};

/// Array of all the implemented built-in utilities.
///
/// The array items are ordered alphabetically.
pub const BUILTINS: &[(&str, Builtin)] = &[
    (
        "alias",
        Builtin {
            r#type: Intrinsic,
            execute: alias::builtin_main,
        },
    ),
    (
        "break",
        Builtin {
            r#type: Special,
            execute: r#break::builtin_main,
        },
    ),
    (
        "continue",
        Builtin {
            r#type: Special,
            execute: r#continue::builtin_main,
        },
    ),
    (
        "jobs",
        Builtin {
            r#type: Intrinsic,
            execute: jobs::builtin_main,
        },
    ),
    (
        "readonly",
        Builtin {
            r#type: Special,
            execute: readonly::builtin_main,
        },
    ),
    (
        "return",
        Builtin {
            r#type: Special,
            execute: r#return::builtin_main,
        },
    ),
    (
        "set",
        Builtin {
            r#type: Special,
            execute: set::builtin_main,
        },
    ),
    (
        "trap",
        Builtin {
            r#type: Special,
            execute: trap::builtin_main,
        },
    ),
    (
        "wait",
        Builtin {
            r#type: Intrinsic,
            execute: wait::builtin_main,
        },
    ),
];

#[cfg(test)]
pub(crate) mod tests {
    use assert_matches::assert_matches;
    use futures_executor::LocalSpawner;
    use futures_util::task::LocalSpawnExt;
    use std::cell::Cell;
    use std::cell::RefCell;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::job::Pid;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::r#virtual::SystemState;
    use yash_env::Env;
    use yash_env::VirtualSystem;

    #[derive(Clone, Debug)]
    pub struct LocalExecutor(pub LocalSpawner);

    impl yash_env::system::r#virtual::Executor for LocalExecutor {
        fn spawn(
            &self,
            task: Pin<Box<dyn Future<Output = ()>>>,
        ) -> Result<(), Box<dyn std::error::Error>> {
            self.0
                .spawn_local(task)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        }
    }

    /// Helper function to perform a test in a virtual system with an executor.
    pub fn in_virtual_system<F, Fut>(f: F)
    where
        F: FnOnce(Env, Pid, Rc<RefCell<SystemState>>) -> Fut,
        Fut: Future<Output = ()> + 'static,
    {
        let system = VirtualSystem::new();
        let pid = system.process_id;
        let state = Rc::clone(&system.state);
        let mut executor = futures_executor::LocalPool::new();
        state.borrow_mut().executor = Some(Rc::new(LocalExecutor(executor.spawner())));

        let env = Env::with_system(Box::new(system));
        let shared_system = env.system.clone();
        let task = f(env, pid, Rc::clone(&state));
        let done = Rc::new(Cell::new(false));
        let done_2 = Rc::clone(&done);

        executor
            .spawner()
            .spawn_local(async move {
                task.await;
                done.set(true);
            })
            .unwrap();

        while !done_2.get() {
            executor.run_until_stalled();
            shared_system.select(false).unwrap();
            SystemState::select_all(&state);
        }
    }

    /// Helper function for asserting on the content of /dev/stdout.
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

    /// Helper function for asserting on the content of /dev/stderr.
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
}
