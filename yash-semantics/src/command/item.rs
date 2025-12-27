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
use crate::trap::run_exit_trap;
use std::ops::ControlFlow::{Break, Continue};
use std::rc::Rc;
use yash_env::Env;
use yash_env::System;
use yash_env::io::Fd;
use yash_env::io::print_error;
use yash_env::job::Job;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::subshell::JobControl;
use yash_env::subshell::Subshell;
use yash_env::system::Close as _;
use yash_env::system::Mode;
use yash_env::system::OfdAccess;
use yash_env::system::Open as _;
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
/// list](yash_env::job::JobList::set_last_async_pid) in the environment.
///
/// Since this function finishes before the asynchronous execution finishes, the
/// exit status does not reflect the results of the and-or list; the exit status
/// is always 0.
///
/// If the [`Monitor`] option is off, the standard input of the asynchronous
/// and-or list is implicitly redirected to `/dev/null`.
///
/// [`Monitor`]: yash_env::option::Option::Monitor
impl<S: System + 'static> Command<S> for syntax::Item {
    async fn execute(&self, env: &mut Env<S>) -> Result {
        match &self.async_flag {
            None => self.and_or.execute(env).await,
            Some(async_flag) => execute_async(env, &self.and_or, async_flag).await,
        }
    }
}

async fn execute_async<S: System + 'static>(
    env: &mut Env<S>,
    and_or: &Rc<AndOrList>,
    async_flag: &Location,
) -> Result {
    let and_or_2 = Rc::clone(and_or);
    let subshell = Subshell::new(|env_2, job_control| {
        Box::pin(async move { async_body(env_2, job_control, &and_or_2).await })
    });
    let subshell = subshell
        .job_control(JobControl::Background)
        .ignore_sigint_sigquit(true);
    match subshell.start(env).await {
        Ok((pid, job_control)) => {
            // remember the process ID as a job
            let mut job = Job::new(pid);
            job.state_changed = false;
            job.name = and_or.to_string();
            if let Some(job_control) = job_control {
                debug_assert_eq!(job_control, JobControl::Background);
                job.job_controlled = true;
            }
            let job_index = env.jobs.add(job);
            env.jobs.set_last_async_pid(pid);

            if env.is_interactive() {
                // report the job number and process ID
                let job_number = job_index + 1;
                let report = format!("[{job_number}] {pid}\n");
                env.system.print_error(&report).await;
            }

            env.exit_status = ExitStatus::SUCCESS;
            Continue(())
        }
        Err(errno) => {
            print_error(
                env,
                "cannot start a subshell to run an asynchronous command".into(),
                errno.to_string().into(),
                async_flag,
            )
            .await;

            Break(Divert::Interrupt(Some(ExitStatus::NOEXEC)))
        }
    }
}

async fn async_body<S: System + 'static>(
    env: &mut Env<S>,
    job_control: Option<JobControl>,
    and_or: &AndOrList,
) {
    if job_control.is_none() {
        nullify_stdin(env).ok();
    }
    let result = and_or.execute(env).await;
    env.apply_result(result);

    run_exit_trap(env).await;
}

fn nullify_stdin<S: System>(env: &mut Env<S>) -> std::result::Result<(), yash_env::system::Errno> {
    env.system.close(Fd::STDIN)?;

    let path = c"/dev/null";
    let fd = env
        .system
        .open(path, OfdAccess::ReadOnly, Default::default(), Mode::empty())?;
    assert_eq!(fd, Fd::STDIN);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::cat_builtin;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use futures_util::FutureExt;
    use futures_util::task::LocalSpawnExt;
    use std::cell::RefCell;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::job::ProcessState;
    use yash_env::option::Option::{Interactive, Monitor};
    use yash_env::option::State::On;
    use yash_env::system::Signals as _;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::r#virtual::Inode;
    use yash_env::system::r#virtual::SystemState;
    use yash_env_test_helper::LocalExecutor;
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;
    use yash_env_test_helper::in_virtual_system;
    use yash_env_test_helper::stub_tty;

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
        let mut env = Env::with_system(system);
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
            _ = item.execute(&mut env).await;

            let job = &env.jobs[0];
            assert!(!job.job_controlled);
            assert!(!job.state_changed);
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
            _ = item.execute(&mut env).await;

            let pids = state.borrow().processes.keys().copied().collect::<Vec<_>>();
            assert_eq!(pids, [env.main_pid, env.jobs.last_async_pid()]);
        })
    }

    #[test]
    fn item_execute_async_no_report_if_non_interactive() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("return", return_builtin());

            let item = syntax::Item {
                and_or: Rc::new("return -n 42".parse().unwrap()),
                async_flag: Some(Location::dummy("")),
            };
            _ = item.execute(&mut env).await;

            assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
        })
    }

    #[test]
    fn item_execute_async_report_if_interactive() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("return", return_builtin());
            env.options.set(Interactive, On);

            let item = syntax::Item {
                and_or: Rc::new("return -n 42".parse().unwrap()),
                async_flag: Some(Location::dummy("")),
            };
            _ = item.execute(&mut env).await;

            let expected_report = format!("[1] {}\n", env.jobs.last_async_pid());
            assert_stderr(&state, |stderr| assert_eq!(stderr, expected_report));
        })
    }

    #[test]
    fn item_execute_async_fail() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
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

            _ = item.execute(&mut env).await;

            let state = state.borrow();
            let process = &state.processes[&env.jobs.last_async_pid()];
            assert_ne!(process.pgid(), env.main_pgid);
            assert!(env.jobs[0].job_controlled);
        })
    }

    fn ignore_sigttin(env: &mut Env<VirtualSystem>) {
        let signal = env
            .system
            .signal_number_from_name(yash_env::signal::Name::Ttin)
            .unwrap();
        env.traps
            .set_action(
                &mut env.system,
                signal,
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
            .save("/dev/null", Rc::new(RefCell::new(Inode::new([]))))
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

            _ = item.execute(&mut env).await;
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

            _ = item.execute(&mut env).await;
            env.wait_for_subshell(env.jobs.last_async_pid())
                .await
                .unwrap();
            assert_stdout(&state, |stdout| assert_eq!(stdout, "input\n"));
        })
    }
}
