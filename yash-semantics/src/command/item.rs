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
use std::ffi::CStr;
use std::ops::ControlFlow::{Break, Continue};
use std::rc::Rc;
use yash_env::io::print_error;
use yash_env::io::Fd;
use yash_env::job::Job;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::subshell::JobControl;
use yash_env::subshell::Subshell;
use yash_env::Env;
use yash_env::System;
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
/// If the [`Monitor`] option is off, the standard input of the asynchronous
/// and-or list is implicitly redirected to `/dev/null`.
///
/// [`Monitor`]: yash_env::option::Option::Monitor
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
    let subshell = Subshell::new(|env_2, job_control| {
        Box::pin(async move { async_body(env_2, job_control, &and_or_2).await })
    });
    let subshell = subshell
        .job_control(JobControl::Background)
        .ignore_sigint_sigquit(true);
    match subshell.start(env).await {
        Ok((pid, job_control)) => {
            let mut job = Job::new(pid);
            job.name = and_or.to_string();
            if let Some(job_control) = job_control {
                debug_assert_eq!(job_control, JobControl::Background);
                job.job_controlled = true;
            }
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

async fn async_body(env: &mut Env, job_control: Option<JobControl>, and_or: &AndOrList) -> Result {
    if job_control.is_none() {
        nullify_stdin(env).ok();
    }
    and_or.execute(env).await
}

fn nullify_stdin(env: &mut Env) -> std::result::Result<(), yash_env::system::Errno> {
    env.system.close(Fd::STDIN)?;

    use yash_env::system::{Mode, OFlag};
    let path = CStr::from_bytes_with_nul(b"/dev/null\0").unwrap();
    let fd = env.system.open(path, OFlag::O_RDONLY, Mode::empty())?;
    assert_eq!(fd, Fd::STDIN);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use crate::tests::cat_builtin;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use crate::tests::stub_tty;
    use crate::tests::LocalExecutor;
    use futures_util::task::LocalSpawnExt;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use std::rc::Rc;
    use yash_env::job::ProcessState;
    use yash_env::option::Option::Monitor;
    use yash_env::option::State::On;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::r#virtual::INode;
    use yash_env::system::r#virtual::SystemState;
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
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("return", return_builtin());
            env.exit_status = ExitStatus::FAILURE;

            let item = syntax::Item {
                and_or: Rc::new("return -n 42".parse().unwrap()),
                async_flag: Some(Location::dummy("")),
            };
            let result = item.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        })
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
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("return", return_builtin());

            let item = syntax::Item {
                and_or: Rc::new("return  -n  42".parse().unwrap()),
                async_flag: Some(Location::dummy("")),
            };
            item.execute(&mut env).await;

            let job = &env.jobs[0];
            assert!(!job.job_controlled);
            assert!(job.state_changed);
            assert_eq!(job.state, ProcessState::Running);
            assert_eq!(job.pid, env.jobs.last_async_pid());
            assert_eq!(job.name, "return -n 42");
        })
    }

    #[test]
    fn item_execute_async_pid() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("return", return_builtin());

            let item = syntax::Item {
                and_or: Rc::new("return -n 42".parse().unwrap()),
                async_flag: Some(Location::dummy("")),
            };
            item.execute(&mut env).await;

            let pids = state.borrow().processes.keys().copied().collect::<Vec<_>>();
            assert_eq!(pids, [env.main_pid, env.jobs.last_async_pid()]);
        })
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

    #[test]
    fn item_execute_async_background() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("return", return_builtin());
            env.options.set(Monitor, On);
            stub_tty(&state);

            let item = syntax::Item {
                and_or: Rc::new("return  -n  42".parse().unwrap()),
                async_flag: Some(Location::dummy("")),
            };

            item.execute(&mut env).await;

            let state = state.borrow();
            let process = &state.processes[&env.jobs.last_async_pid()];
            assert_ne!(process.pgid(), env.main_pgid);
            assert!(env.jobs[0].job_controlled);
        })
    }

    fn ignore_sigttin(env: &mut Env) {
        env.traps
            .set_action(
                &mut env.system,
                yash_env::trap::Signal::SIGTTIN,
                yash_env::trap::Action::Ignore,
                Location::dummy(""),
                false,
            )
            .unwrap();
    }

    fn stub_dev_null_and_stdin(state: &RefCell<SystemState>) {
        let mut state = state.borrow_mut();
        state
            .file_system
            .save("/dev/null", Rc::new(RefCell::new(INode::new([]))))
            .unwrap();
        state
            .file_system
            .get("/dev/stdin")
            .unwrap()
            .borrow_mut()
            .body = FileBody::new(*b"input\n");
    }

    #[test]
    fn item_execute_async_stdin_not_job_controlled() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("cat", cat_builtin());
            ignore_sigttin(&mut env);
            stub_tty(&state);
            stub_dev_null_and_stdin(&state);

            let item = syntax::Item {
                and_or: Rc::new("cat".parse().unwrap()),
                async_flag: Some(Location::dummy("")),
            };

            item.execute(&mut env).await;
            env.wait_for_subshell(env.jobs.last_async_pid())
                .await
                .unwrap();
            assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        })
    }

    #[test]
    fn item_execute_async_stdin_job_controlled() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("cat", cat_builtin());
            env.options.set(Monitor, On);
            ignore_sigttin(&mut env);
            stub_tty(&state);
            stub_dev_null_and_stdin(&state);

            let item = syntax::Item {
                and_or: Rc::new("cat".parse().unwrap()),
                async_flag: Some(Location::dummy("")),
            };

            item.execute(&mut env).await;
            env.wait_for_subshell(env.jobs.last_async_pid())
                .await
                .unwrap();
            assert_stdout(&state, |stdout| assert_eq!(stdout, "input\n"));
        })
    }
}
