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

//! Implementation for Item.

use super::Command;
use async_trait::async_trait;
use std::ops::ControlFlow::{Break, Continue};
use std::rc::Rc;
use yash_env::io::print_error;
use yash_env::job::Job;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::subshell::Subshell;
use yash_env::Env;
use yash_syntax::source::Location;
use yash_syntax::syntax;
use yash_syntax::syntax::AndOrList;

/// Executes the item.
///
/// # Synchronous command
///
/// If the item's `async_flag` is `None`, this function executes the and-or list
/// in the item.
///
/// # Asynchronous command
///
/// If the item has an `async_flag` set, the and-or list is executed
/// asynchronously in a subshell, whose process ID is [set to the job
/// set](yash_env::job::JobSet::set_last_async_pid) in the environment.
///
/// Since this function finishes before the asynchronous execution finishes, the
/// exit status does not reflect the results of the and-or list; the exit status
/// is always 0.
///
/// TODO: If the `monitor` option is off, the standard input of the asynchronous
/// and-or list is implicitly redirected to `/dev/null`.
#[async_trait(?Send)]
impl Command for syntax::Item {
    async fn execute(&self, env: &mut Env) -> Result {
        match &self.async_flag {
            None => self.and_or.execute(env).await,
            Some(async_flag) => execute_async(env, &self.and_or, async_flag).await,
        }
    }
}

async fn execute_async(env: &mut Env, and_or: &Rc<AndOrList>, async_flag: &Location) -> Result {
    let and_or_2 = Rc::clone(and_or);
    let subshell = Subshell::new(|env| Box::pin(async move { and_or_2.execute(env).await }));
    let result = subshell.start(env).await;
    match result {
        Ok((pid, job_control)) => {
            debug_assert_eq!(job_control, None);
            let mut job = Job::new(pid);
            job.name = and_or.to_string();
            env.jobs.add(job);
            env.jobs.set_last_async_pid(pid);
            env.exit_status = ExitStatus::SUCCESS;
            Continue(())
        }
        Err(errno) => {
            print_error(
                env,
                "cannot start a subshell to run an asynchronous command".into(),
                errno.desc().into(),
                async_flag,
            )
            .await;

            Break(Divert::Interrupt(Some(ExitStatus::NOEXEC)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use crate::tests::LocalExecutor;
    use futures_util::task::LocalSpawnExt;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use yash_env::job::WaitStatus;
    use yash_env::VirtualSystem;

    #[test]
    fn item_execute_sync() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let and_or: syntax::AndOrList = "return -n 42".parse().unwrap();
        let item = syntax::Item {
            and_or: Rc::new(and_or),
            async_flag: None,
        };
        let result = item.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(42));
    }

    #[test]
    fn item_execute_async_exit_status() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut executor = futures_executor::LocalPool::new();
        state.borrow_mut().executor = Some(Rc::new(LocalExecutor(executor.spawner())));
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus::FAILURE;

        let and_or: syntax::AndOrList = "return -n 42".parse().unwrap();
        let item = syntax::Item {
            and_or: Rc::new(and_or),
            async_flag: Some(Location::dummy("")),
        };
        let result = executor.run_until(item.execute(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
    }

    #[test]
    fn item_execute_async_effect() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut executor = futures_executor::LocalPool::new();
        state.borrow_mut().executor = Some(Rc::new(LocalExecutor(executor.spawner())));
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());

        let and_or: syntax::AndOrList = "echo foo".parse().unwrap();
        let item = syntax::Item {
            and_or: Rc::new(and_or),
            async_flag: Some(Location::dummy("")),
        };

        executor
            .spawner()
            .spawn_local(async move {
                let result = item.execute(&mut env).await;
                assert_eq!(result, Continue(()));
            })
            .unwrap();
        executor.run_until_stalled();

        assert_stdout(&state, |stdout| assert_eq!(stdout, "foo\n"));
    }

    #[test]
    fn item_execute_async_job() {
        let system = VirtualSystem::new();
        let mut executor = futures_executor::LocalPool::new();
        system.state.borrow_mut().executor = Some(Rc::new(LocalExecutor(executor.spawner())));
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("return", return_builtin());

        let and_or: syntax::AndOrList = "return  -n  42".parse().unwrap();
        let item = syntax::Item {
            and_or: Rc::new(and_or),
            async_flag: Some(Location::dummy("")),
        };
        executor.run_until(item.execute(&mut env));

        let job = env.jobs.get(0).unwrap();
        assert!(job.status_changed);
        assert_eq!(job.status, WaitStatus::StillAlive);
        assert_eq!(job.name, "return -n 42");
    }

    #[test]
    fn item_execute_async_pid() {
        let system = VirtualSystem::new();
        let main_pid = system.process_id;
        let state = Rc::clone(&system.state);
        let mut executor = futures_executor::LocalPool::new();
        state.borrow_mut().executor = Some(Rc::new(LocalExecutor(executor.spawner())));
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("return", return_builtin());

        let and_or: syntax::AndOrList = "return -n 42".parse().unwrap();
        let item = syntax::Item {
            and_or: Rc::new(and_or),
            async_flag: Some(Location::dummy("")),
        };
        executor.run_until(item.execute(&mut env));

        let pids = state.borrow().processes.keys().copied().collect::<Vec<_>>();
        assert_eq!(pids, [main_pid, env.jobs.last_async_pid()]);
    }

    #[test]
    fn item_execute_async_fail() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("return", return_builtin());

        let and_or: syntax::AndOrList = "return -n 42".parse().unwrap();
        let item = syntax::Item {
            and_or: Rc::new(and_or),
            async_flag: Some(Location::dummy("X")),
        };
        let result = item.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::NOEXEC))));
        assert_stderr(&state, |stderr| {
            assert!(
                stderr.contains("asynchronous"),
                "unexpected error message: {stderr:?}"
            )
        });
    }
}
